//! This device's copy of the user's notification mute set (notification-prefs:
//! Mute preference sync, change `mute-notifications`).
//!
//! The set holds the muted conversation ids and the `muted_all` flag. It is
//! adopted wholesale from each `MutePrefs` snapshot the server sends on connect,
//! and updated locally when this device's own `SetMutePref` succeeds. Ids and a
//! flag only; the app reads it to suppress its own in-app notification and to
//! mark muted rows.

use rusqlite::params;

use super::{Store, StoreError};

impl Store {
    /// Replaces the whole mute set with a `MutePrefs` snapshot (adopted
    /// wholesale: a device catches up on every mute/unmute it missed in one
    /// frame). Returns whether the set changed.
    pub fn set_mute_snapshot(
        &self,
        muted_group_ids: &[Vec<u8>],
        muted_all: bool,
    ) -> Result<bool, StoreError> {
        self.transaction(|| {
            let before = (self.muted_group_ids()?, self.muted_all()?);
            self.db.with(|conn| -> Result<(), StoreError> {
                conn.execute("DELETE FROM muted_conversations", [])?;
                for group_id in muted_group_ids {
                    conn.execute(
                        "INSERT INTO muted_conversations (group_id) VALUES (?1)
                         ON CONFLICT DO NOTHING",
                        params![group_id],
                    )?;
                }
                set_all(conn, muted_all)?;
                Ok(())
            })?;
            Ok((before.0, before.1) != (self.muted_group_ids()?, self.muted_all()?))
        })
    }

    /// Records this device's own mute/unmute of one conversation, after the
    /// server accepted it. Idempotent.
    pub fn set_conversation_muted(
        &self,
        group_id: &[u8],
        muted: bool,
    ) -> Result<(), StoreError> {
        self.db.with(|conn| {
            if muted {
                conn.execute(
                    "INSERT INTO muted_conversations (group_id) VALUES (?1)
                     ON CONFLICT DO NOTHING",
                    params![group_id],
                )?;
            } else {
                conn.execute(
                    "DELETE FROM muted_conversations WHERE group_id = ?1",
                    params![group_id],
                )?;
            }
            Ok(())
        })
    }

    /// Records this device's own whole-app mute, after the server accepted it.
    pub fn set_muted_all(&self, muted: bool) -> Result<(), StoreError> {
        self.db.with(|conn| set_all(conn, muted))
    }

    /// Whether `group_id` is in the muted set (per-conversation only; does not
    /// consult `muted_all`).
    pub fn is_conversation_muted(&self, group_id: &[u8]) -> Result<bool, StoreError> {
        self.db.with(|conn| {
            Ok(conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM muted_conversations WHERE group_id = ?1)",
                params![group_id],
                |row| row.get(0),
            )?)
        })
    }

    /// Whether whole-app mute is set.
    pub fn muted_all(&self) -> Result<bool, StoreError> {
        self.db.with(|conn| {
            Ok(conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM muted_all WHERE id = 1)",
                [],
                |row| row.get(0),
            )?)
        })
    }

    /// The muted conversation ids, sorted.
    pub fn muted_group_ids(&self) -> Result<Vec<Vec<u8>>, StoreError> {
        self.db.with(|conn| {
            let mut stmt =
                conn.prepare("SELECT group_id FROM muted_conversations ORDER BY group_id")?;
            let rows = stmt.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
            let mut ids = Vec::new();
            for group_id in rows {
                ids.push(group_id?);
            }
            Ok(ids)
        })
    }
}

/// Sets or clears the single `muted_all` row.
fn set_all(conn: &rusqlite::Connection, muted: bool) -> Result<(), StoreError> {
    if muted {
        conn.execute(
            "INSERT INTO muted_all (id) VALUES (1) ON CONFLICT DO NOTHING",
            [],
        )?;
    } else {
        conn.execute("DELETE FROM muted_all WHERE id = 1", [])?;
    }
    Ok(())
}
