//! This device's account, its bookkeeping for the group, KeyPackage records and
//! the own commit in flight (client-core task groups 4 and 5). No keys and no
//! plaintext live here.

use std::time::SystemTime;

use rusqlite::{OptionalExtension, params};

use super::{Store, StoreError};
use crate::clock::{from_unix_millis, unix_millis};
use crate::events::DeviceId;

/// The account this device registered or was linked into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRow {
    /// Where the Pi is reached (the relay), from the invite or link approval.
    pub relay_address: String,
    /// SHA-256 of the Pi's TLS SubjectPublicKeyInfo.
    pub tls_pin: [u8; 32],
    /// This device's user: username.
    pub username: String,
    /// This device's user: server id.
    pub server_id: String,
    /// This device.
    pub device_id: DeviceId,
    /// `SignedDeviceCertificate.certificate`, as carried.
    pub certificate: Vec<u8>,
    /// `SignedDeviceCertificate.signature`.
    pub certificate_signature: Vec<u8>,
    /// Whether this user is the admin.
    pub is_admin: bool,
    /// `ServerInfo.external_sender.signature_public_key`.
    pub pi_sender_key: Vec<u8>,
    /// `ServerInfo.external_sender.credential`.
    pub pi_sender_credential: Vec<u8>,
    /// When this device got its account.
    pub created_at: SystemTime,
}

/// This device's bookkeeping for the group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupRow {
    /// The MLS group id.
    pub group_id: Vec<u8>,
    /// When this device's leaf was last replaced.
    pub own_leaf_updated_at: SystemTime,
    /// The highest `server_seq` durably processed.
    pub processed_seq: u64,
    /// Why this device stopped following the group, if it did.
    pub stalled: Option<String>,
}

/// The own commit in flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingCommitRow {
    /// Its `msg_id`.
    pub msg_id: [u8; 16],
    /// The encoded `Envelope`, exactly as sent.
    pub envelope: Vec<u8>,
    /// What to finish once it is accepted.
    pub purpose: String,
}

fn seq_to_sql(seq: u64) -> i64 {
    i64::try_from(seq).unwrap_or(i64::MAX)
}

fn fixed<const N: usize>(bytes: Vec<u8>, what: &str) -> Result<[u8; N], StoreError> {
    bytes
        .try_into()
        .map_err(|_| StoreError::Inconsistent(format!("{what} has the wrong length")))
}

impl Store {
    /// The account, once this device registered or was linked.
    pub fn account(&self) -> Result<Option<AccountRow>, StoreError> {
        let row = self.db.with(|conn| {
            conn.query_row(
                "SELECT relay_address, tls_pin, username, server_id, device_id, certificate,
                        certificate_signature, is_admin, pi_sender_key, pi_sender_credential,
                        created_at
                 FROM account WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Vec<u8>>(4)?,
                        row.get::<_, Vec<u8>>(5)?,
                        row.get::<_, Vec<u8>>(6)?,
                        row.get::<_, bool>(7)?,
                        row.get::<_, Vec<u8>>(8)?,
                        row.get::<_, Vec<u8>>(9)?,
                        row.get::<_, i64>(10)?,
                    ))
                },
            )
            .optional()
        })?;
        let Some((relay, pin, username, server_id, device, cert, sig, admin, key, cred, at)) = row
        else {
            return Ok(None);
        };
        Ok(Some(AccountRow {
            relay_address: relay,
            tls_pin: fixed(pin, "tls_pin")?,
            username,
            server_id,
            device_id: fixed(device, "device_id")?,
            certificate: cert,
            certificate_signature: sig,
            is_admin: admin,
            pi_sender_key: key,
            pi_sender_credential: cred,
            created_at: from_unix_millis(at),
        }))
    }

    /// Records the account. A device has one account for the life of its
    /// database: a second one is an error.
    pub fn save_account(&self, account: &AccountRow) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO account (id, relay_address, tls_pin, username, server_id, device_id,
                     certificate, certificate_signature, is_admin, pi_sender_key,
                     pi_sender_credential, created_at)
                 VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    account.relay_address,
                    account.tls_pin.as_slice(),
                    account.username,
                    account.server_id,
                    account.device_id.as_slice(),
                    account.certificate,
                    account.certificate_signature,
                    account.is_admin,
                    account.pi_sender_key,
                    account.pi_sender_credential,
                    unix_millis(account.created_at),
                ],
            )
        })?;
        Ok(())
    }

    /// Updates the admin flag (from `Devices.GetSelf`).
    pub fn set_admin(&self, is_admin: bool) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE account SET is_admin = ?1 WHERE id = 1",
                params![is_admin],
            )
        })?;
        Ok(())
    }

    /// This device's bookkeeping for the group, once it has one.
    pub fn group(&self) -> Result<Option<GroupRow>, StoreError> {
        let row = self.db.with(|conn| {
            conn.query_row(
                "SELECT group_id, own_leaf_updated_at, processed_seq, stalled
                 FROM group_state WHERE id = 1",
                [],
                |row| {
                    Ok(GroupRow {
                        group_id: row.get(0)?,
                        own_leaf_updated_at: from_unix_millis(row.get(1)?),
                        processed_seq: u64::try_from(row.get::<_, i64>(2)?).unwrap_or(0),
                        stalled: row.get(3)?,
                    })
                },
            )
            .optional()
        })?;
        Ok(row)
    }

    /// Records that this device created or (re)joined the group with a fresh
    /// leaf at `now`, having processed everything up to `processed_seq`.
    /// Clears a stall: a Welcome is a fresh start.
    pub fn set_group(
        &self,
        group_id: &[u8],
        now: SystemTime,
        processed_seq: u64,
    ) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO group_state (id, group_id, own_leaf_updated_at, processed_seq, stalled)
                 VALUES (1, ?1, ?2, ?3, NULL)
                 ON CONFLICT (id) DO UPDATE SET group_id = ?1, own_leaf_updated_at = ?2,
                     processed_seq = max(processed_seq, ?3), stalled = NULL",
                params![group_id, unix_millis(now), seq_to_sql(processed_seq)],
            )
        })?;
        Ok(())
    }

    /// Forgets the group (a failed `Admin.CreateGroup`).
    pub fn clear_group(&self) -> Result<(), StoreError> {
        self.db
            .with(|conn| conn.execute("DELETE FROM group_state", []))?;
        Ok(())
    }

    /// This device's leaf was replaced at `now` (an own commit with a path).
    pub fn set_own_leaf_updated(&self, now: SystemTime) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE group_state SET own_leaf_updated_at = ?1 WHERE id = 1",
                params![unix_millis(now)],
            )
        })?;
        Ok(())
    }

    /// Everything up to `seq` is durably processed.
    pub fn set_processed_seq(&self, seq: u64) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE group_state SET processed_seq = max(processed_seq, ?1) WHERE id = 1",
                params![seq_to_sql(seq)],
            )
        })?;
        Ok(())
    }

    /// This device stopped following the group, because of `why`.
    pub fn set_stalled(&self, why: &str) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE group_state SET stalled = ?1 WHERE id = 1",
                params![why],
            )
        })?;
        Ok(())
    }

    /// The own commit in flight, if any.
    pub fn outgoing_commit(&self) -> Result<Option<OutgoingCommitRow>, StoreError> {
        let row = self.db.with(|conn| {
            conn.query_row(
                "SELECT msg_id, envelope, purpose FROM outgoing_commit WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
        })?;
        row.map(|(msg_id, envelope, purpose)| {
            Ok(OutgoingCommitRow {
                msg_id: fixed(msg_id, "outgoing msg_id")?,
                envelope,
                purpose,
            })
        })
        .transpose()
    }

    /// Records the own commit about to be sent (replacing none: there is at
    /// most one).
    pub fn set_outgoing_commit(&self, row: &OutgoingCommitRow) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO outgoing_commit (id, msg_id, envelope, purpose) VALUES (1, ?1, ?2, ?3)",
                params![row.msg_id.as_slice(), row.envelope, row.purpose],
            )
        })?;
        Ok(())
    }

    /// The own commit is no longer in flight.
    pub fn clear_outgoing_commit(&self) -> Result<(), StoreError> {
        self.db
            .with(|conn| conn.execute("DELETE FROM outgoing_commit", []))?;
        Ok(())
    }

    /// Records an uploaded last-resort KeyPackage. Returns the hash refs of
    /// the ones now two or more refreshes old, whose private keys can go.
    pub fn add_last_resort(
        &self,
        hash_ref: &[u8],
        now: SystemTime,
    ) -> Result<Vec<Vec<u8>>, StoreError> {
        self.transaction(|| {
            self.db.with(|conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO last_resort_key_packages (hash_ref, uploaded_at)
                     VALUES (?1, ?2)",
                    params![hash_ref, unix_millis(now)],
                )
            })?;
            let old: Vec<Vec<u8>> = self.db.with(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT hash_ref FROM last_resort_key_packages
                     ORDER BY uploaded_at DESC, rowid DESC LIMIT -1 OFFSET 2",
                )?;
                let rows = stmt.query_map([], |row| row.get(0))?;
                rows.collect::<Result<Vec<_>, _>>()
            })?;
            for hash in &old {
                self.db.with(|conn| {
                    conn.execute(
                        "DELETE FROM last_resort_key_packages WHERE hash_ref = ?1",
                        params![hash],
                    )
                })?;
            }
            Ok(old)
        })
    }

    /// When the newest last-resort KeyPackage was uploaded.
    pub fn last_resort_uploaded_at(&self) -> Result<Option<SystemTime>, StoreError> {
        let at: Option<i64> = self.db.with(|conn| {
            conn.query_row(
                "SELECT max(uploaded_at) FROM last_resort_key_packages",
                [],
                |row| row.get(0),
            )
        })?;
        Ok(at.map(from_unix_millis))
    }

    /// Records a device this device linked, to add to the group.
    pub fn add_pending_link(
        &self,
        device_id: &DeviceId,
        now: SystemTime,
    ) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT OR IGNORE INTO pending_links (device_id, created_at) VALUES (?1, ?2)",
                params![device_id.as_slice(), unix_millis(now)],
            )
        })?;
        Ok(())
    }

    /// Linked devices not yet added, oldest first.
    pub fn pending_links(&self) -> Result<Vec<DeviceId>, StoreError> {
        let ids: Vec<Vec<u8>> = self.db.with(|conn| {
            let mut stmt =
                conn.prepare("SELECT device_id FROM pending_links ORDER BY created_at, rowid")?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            rows.collect::<Result<Vec<_>, _>>()
        })?;
        ids.into_iter()
            .map(|id| fixed(id, "pending link device id"))
            .collect()
    }

    /// A linked device was added (or can never be).
    pub fn remove_pending_link(&self, device_id: &DeviceId) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "DELETE FROM pending_links WHERE device_id = ?1",
                params![device_id.as_slice()],
            )
        })?;
        Ok(())
    }
}
