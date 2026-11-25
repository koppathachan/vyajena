//! key-directory: Clients verify the identity chain. Identity keys are pinned
//! on first sight; a different key for a known user waits until the user
//! accepts it (client-core design D5).

use std::time::SystemTime;

use rusqlite::{OptionalExtension, params};

use super::{Store, StoreError};
use crate::clock::{from_unix_millis, unix_millis};

/// What [`Store::pin_identity_key`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinOutcome {
    /// First sight of this user: the key is now pinned.
    Pinned,
    /// The key matches the pinned one.
    Matches,
    /// The key differs from the pinned one. It is kept as pending; the core
    /// raises an identity-key-changed event and adds none of the user's
    /// devices until the user accepts it ([`Store::accept_identity_key`]).
    Changed,
}

/// A pinned identity key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedIdentityKey {
    /// The pinned key.
    pub key: Vec<u8>,
    /// When it was pinned (or accepted).
    pub pinned_at: SystemTime,
    /// A different key seen since, waiting for the user.
    pub pending: Option<Vec<u8>>,
    /// Set when the pinned key was auto-repinned to keep the group decrypting
    /// (a reset user or the recovered admin re-keyed): the identity is
    /// unverified / pending re-trust until the owner acknowledges it. `None`
    /// means the pinned key is trusted (key-directory: Clients verify the
    /// identity chain; admin-recovery: Admin re-key forces peer re-trust).
    pub retrust_since: Option<SystemTime>,
}

impl PinnedIdentityKey {
    /// Whether the owner must still re-trust this identity: a different key is
    /// waiting, or the pinned key was auto-repinned and not yet acknowledged.
    pub fn needs_retrust(&self) -> bool {
        self.pending.is_some() || self.retrust_since.is_some()
    }
}

impl Store {
    /// Pins `key` for `user_id` on first sight, or compares it with the pinned
    /// key (key-directory: Clients verify the identity chain).
    pub fn pin_identity_key(
        &self,
        user_id: &str,
        key: &[u8],
        now: SystemTime,
    ) -> Result<PinOutcome, StoreError> {
        self.transaction(|| {
            let Some(pinned) = self.pinned_identity_key(user_id)? else {
                self.db.with(|conn| {
                    conn.execute(
                        "INSERT INTO identity_keys (user_id, identity_key, pinned_at)
                         VALUES (?1, ?2, ?3)",
                        params![user_id, key, unix_millis(now)],
                    )
                })?;
                tracing::info!(user = user_id, "pinned identity key");
                return Ok(PinOutcome::Pinned);
            };
            if pinned.key == key {
                return Ok(PinOutcome::Matches);
            }
            if pinned.pending.as_deref() != Some(key) {
                self.db.with(|conn| {
                    conn.execute(
                        "UPDATE identity_keys SET pending_key = ?2, pending_since = ?3
                         WHERE user_id = ?1",
                        params![user_id, key, unix_millis(now)],
                    )
                })?;
                tracing::warn!(user = user_id, "identity key changed");
            }
            Ok(PinOutcome::Changed)
        })
    }

    /// The pinned identity key of `user_id`, if any.
    pub fn pinned_identity_key(
        &self,
        user_id: &str,
    ) -> Result<Option<PinnedIdentityKey>, StoreError> {
        let row = self.db.with(|conn| {
            conn.query_row(
                "SELECT identity_key, pinned_at, pending_key, retrust_since FROM identity_keys
                 WHERE user_id = ?1",
                params![user_id],
                |row| {
                    Ok(PinnedIdentityKey {
                        key: row.get(0)?,
                        pinned_at: from_unix_millis(row.get(1)?),
                        pending: row.get(2)?,
                        retrust_since: row.get::<_, Option<i64>>(3)?.map(from_unix_millis),
                    })
                },
            )
            .optional()
        })?;
        Ok(row)
    }

    /// The user ids whose identity is unverified / pending re-trust: a key is
    /// waiting to be accepted, or the pinned key was auto-repinned and not yet
    /// acknowledged (admin-recovery: Admin re-key forces peer re-trust).
    pub fn identity_retrust_pending(&self) -> Result<Vec<String>, StoreError> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT user_id FROM identity_keys
                 WHERE pending_key IS NOT NULL OR retrust_since IS NOT NULL
                 ORDER BY user_id",
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
    }

    /// Clears the pending-re-trust mark of `user_id` after the owner
    /// acknowledges the changed safety number (admin-recovery: Admin re-key
    /// forces peer re-trust). Returns whether a mark was cleared. It does not
    /// touch a waiting `pending_key` (that is promoted by [`accept_identity_key`]).
    pub fn acknowledge_retrust(&self, user_id: &str, _now: SystemTime) -> Result<bool, StoreError> {
        let changed = self.db.with(|conn| {
            conn.execute(
                "UPDATE identity_keys SET retrust_since = NULL
                 WHERE user_id = ?1 AND retrust_since IS NOT NULL",
                params![user_id],
            )
        })?;
        if changed == 1 {
            tracing::info!(user = user_id, "re-trusted a changed identity key");
        }
        Ok(changed == 1)
    }

    /// The user accepted `key`, the pending key of `user_id`, in the UI: it
    /// becomes the pinned key. Returns `false`, changing nothing, if `key` is
    /// not the pending key (so a key the user never saw cannot be accepted).
    pub fn accept_identity_key(
        &self,
        user_id: &str,
        key: &[u8],
        now: SystemTime,
    ) -> Result<bool, StoreError> {
        let changed = self.db.with(|conn| {
            conn.execute(
                "UPDATE identity_keys
                 SET identity_key = pending_key, pinned_at = ?3,
                     pending_key = NULL, pending_since = NULL, retrust_since = NULL
                 WHERE user_id = ?1 AND pending_key = ?2",
                params![user_id, key, unix_millis(now)],
            )
        })?;
        if changed == 1 {
            tracing::info!(user = user_id, "accepted new identity key");
        }
        Ok(changed == 1)
    }

    /// Replaces the pinned key of `user_id` with `key` without the user
    /// accepting it first: only for a key the server confirmed for a device the
    /// admin added (key-directory: Clients verify the identity chain, "Admin
    /// re-adds a reset user"). The new key is pinned so the group keeps
    /// decrypting, but the identity is marked **unverified / pending re-trust**
    /// (`retrust_since = now`): the client surfaces a safety-number-changed
    /// prompt and withholds the verified state until the owner acknowledges it
    /// ([`Store::acknowledge_retrust`]). It never silently auto-accepts the
    /// changed key as trusted (admin-recovery: Admin re-key forces peer
    /// re-trust).
    pub fn repin_identity_key(
        &self,
        user_id: &str,
        key: &[u8],
        now: SystemTime,
    ) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO identity_keys (user_id, identity_key, pinned_at, retrust_since)
                 VALUES (?1, ?2, ?3, ?3)
                 ON CONFLICT (user_id) DO UPDATE
                 SET identity_key = excluded.identity_key, pinned_at = excluded.pinned_at,
                     pending_key = NULL, pending_since = NULL, retrust_since = ?3",
                params![user_id, key, unix_millis(now)],
            )
        })?;
        tracing::warn!(
            user = user_id,
            "re-pinned a changed identity key; marked pending re-trust"
        );
        Ok(())
    }
}
