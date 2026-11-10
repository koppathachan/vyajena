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
                "SELECT identity_key, pinned_at, pending_key FROM identity_keys
                 WHERE user_id = ?1",
                params![user_id],
                |row| {
                    Ok(PinnedIdentityKey {
                        key: row.get(0)?,
                        pinned_at: from_unix_millis(row.get(1)?),
                        pending: row.get(2)?,
                    })
                },
            )
            .optional()
        })?;
        Ok(row)
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
                     pending_key = NULL, pending_since = NULL
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
    /// accepting it first: only for a key the Pi confirmed for a device the
    /// admin added (key-directory: Clients verify the identity chain, "Admin
    /// re-adds a reset user"). The UI is warned that the key changed.
    pub fn repin_identity_key(
        &self,
        user_id: &str,
        key: &[u8],
        now: SystemTime,
    ) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO identity_keys (user_id, identity_key, pinned_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT (user_id) DO UPDATE
                 SET identity_key = excluded.identity_key, pinned_at = excluded.pinned_at,
                     pending_key = NULL, pending_since = NULL",
                params![user_id, key, unix_millis(now)],
            )
        })?;
        tracing::warn!(user = user_id, "re-pinned a changed identity key");
        Ok(())
    }
}
