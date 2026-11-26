//! This device's view of manual presence (manual-presence: Setting your status;
//! Latest value wins, change `manual-presence`).
//!
//! Two pieces of state: this user's own flag (with a `dirty` bit so a toggle made
//! while offline is sent on the next connect), and the latest flag learned for
//! each other user, kept latest-`updated_at`-wins.

use protocol::presence::Presence;
use rusqlite::{OptionalExtension as _, params};

use super::{Store, StoreError};

/// This user's own flag and whether it still needs sending to the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SelfPresence {
    /// The user's chosen flag.
    pub status: Presence,
    /// Whether a change the user made has not yet reached the server.
    pub dirty: bool,
}

fn presence_from_wire(value: i32) -> Result<Presence, StoreError> {
    Presence::from_wire(value)
        .ok_or_else(|| StoreError::Inconsistent(format!("bad stored presence value {value}")))
}

impl Store {
    /// This user's own manual flag and its dirty bit.
    pub(crate) fn self_presence(&self) -> Result<SelfPresence, StoreError> {
        self.db.with(|conn| {
            let (status, dirty): (i32, bool) = conn.query_row(
                "SELECT status, dirty FROM self_presence WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            Ok(SelfPresence {
                status: presence_from_wire(status)?,
                dirty,
            })
        })
    }

    /// Records the user's own choice and marks it to send (dirty). Returns
    /// whether the stored flag changed.
    pub fn set_self_presence(&self, status: Presence) -> Result<bool, StoreError> {
        self.db.with(|conn| {
            let before: i32 =
                conn.query_row("SELECT status FROM self_presence WHERE id = 1", [], |row| {
                    row.get(0)
                })?;
            conn.execute(
                "UPDATE self_presence SET status = ?1, dirty = 1 WHERE id = 1",
                params![status.wire_value()],
            )?;
            Ok(before != status.wire_value())
        })
    }

    /// Clears the dirty bit after the current own flag was sent to the server.
    pub fn clear_self_presence_dirty(&self) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute("UPDATE self_presence SET dirty = 0 WHERE id = 1", [])?;
            Ok(())
        })
    }

    /// The own flag to send on connect, if one is pending (dirty); `None`
    /// otherwise, so a mere reconnect does not re-assert and clobber a newer
    /// change from another of the user's devices.
    pub fn pending_self_presence(&self) -> Result<Option<Presence>, StoreError> {
        let self_presence = self.self_presence()?;
        Ok(self_presence.dirty.then_some(self_presence.status))
    }

    /// manual-presence: Latest value wins. Applies a `PresenceUpdate` for `user`
    /// (`<username>@<server_id>`) only if `updated_at_ms` is newer than the stored
    /// one; returns whether the stored flag changed (so the UI updates once).
    pub fn apply_peer_presence(
        &self,
        user: &str,
        status: Presence,
        updated_at_ms: i64,
    ) -> Result<bool, StoreError> {
        self.db.with(|conn| {
            let current: Option<(i32, i64)> = conn
                .query_row(
                    "SELECT status, updated_at_ms FROM peer_presence WHERE user_id = ?1",
                    params![user],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            if let Some((_, stored_at)) = current
                && updated_at_ms <= stored_at
            {
                return Ok(false);
            }
            let changed = current.map(|(s, _)| s) != Some(status.wire_value());
            conn.execute(
                "INSERT INTO peer_presence (user_id, status, updated_at_ms) VALUES (?1, ?2, ?3)
                 ON CONFLICT (user_id) DO UPDATE SET status = ?2, updated_at_ms = ?3",
                params![user, status.wire_value(), updated_at_ms],
            )?;
            Ok(changed)
        })
    }

    /// The latest known flag for `user` (`<username>@<server_id>`), or `None`
    /// when this device has learned none (shows no dot).
    pub fn peer_presence(&self, user: &str) -> Result<Option<Presence>, StoreError> {
        self.db.with(|conn| {
            let status: Option<i32> = conn
                .query_row(
                    "SELECT status FROM peer_presence WHERE user_id = ?1",
                    params![user],
                    |row| row.get(0),
                )
                .optional()?;
            status.map(presence_from_wire).transpose()
        })
    }
}
