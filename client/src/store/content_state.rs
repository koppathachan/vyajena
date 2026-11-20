//! The state that message bodies change (message-content: User-owned
//! profiles; Group settings travel encrypted): each user's last profile and
//! the group's settings.

use std::time::SystemTime;

use rusqlite::{OptionalExtension, params};

use super::{Store, StoreError};
use crate::clock::unix_millis;

/// A user's profile as their devices last set it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileRow {
    /// The display name, if set.
    pub display_name: Option<String>,
    /// The "about" text, if set.
    pub about: Option<String>,
}

impl Store {
    /// Sets the profile of `user_id`, `<username>@<server_id>`: the user
    /// whose device sent it, never another one.
    pub fn set_profile(
        &self,
        user_id: &str,
        profile: &ProfileRow,
        now: SystemTime,
    ) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO profiles (user_id, display_name, about, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (user_id) DO UPDATE
                 SET display_name = excluded.display_name, about = excluded.about,
                     updated_at = excluded.updated_at",
                params![
                    user_id,
                    profile.display_name,
                    profile.about,
                    unix_millis(now)
                ],
            )
        })?;
        Ok(())
    }

    /// Drops the stored profile of `user_id` (message-content: User-owned
    /// profiles: their identity key changed). Returns whether one was stored.
    pub fn delete_profile(&self, user_id: &str) -> Result<bool, StoreError> {
        let deleted = self.db.with(|conn| {
            conn.execute("DELETE FROM profiles WHERE user_id = ?1", params![user_id])
        })?;
        Ok(deleted > 0)
    }

    /// Records that the server's inactivity Remove took `device_id` out of the
    /// group (qa-fixes D5).
    pub fn mark_inactive(&self, device_id: &[u8], now: SystemTime) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO inactive_devices (device_id, removed_at) VALUES (?1, ?2)
                 ON CONFLICT (device_id) DO UPDATE SET removed_at = excluded.removed_at",
                params![device_id, unix_millis(now)],
            )
        })?;
        Ok(())
    }

    /// `device_id` is in the group again: forgets its inactivity Remove and
    /// returns whether there was one (its add is then a re-add).
    pub fn take_inactive(&self, device_id: &[u8]) -> Result<bool, StoreError> {
        let deleted = self.db.with(|conn| {
            conn.execute(
                "DELETE FROM inactive_devices WHERE device_id = ?1",
                params![device_id],
            )
        })?;
        Ok(deleted > 0)
    }

    /// The profile of `user_id`, if their devices ever sent one.
    pub fn profile(&self, user_id: &str) -> Result<Option<ProfileRow>, StoreError> {
        Ok(self.db.with(|conn| {
            conn.query_row(
                "SELECT display_name, about FROM profiles WHERE user_id = ?1",
                params![user_id],
                |row| {
                    Ok(ProfileRow {
                        display_name: row.get(0)?,
                        about: row.get(1)?,
                    })
                },
            )
            .optional()
        })?)
    }

    /// Sets the group's name from an admin device's `GroupSettings`.
    pub fn set_group_name(&self, name: Option<&str>, now: SystemTime) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO group_settings (id, name, updated_at) VALUES (1, ?1, ?2)
                 ON CONFLICT (id) DO UPDATE
                 SET name = excluded.name, updated_at = excluded.updated_at",
                params![name, unix_millis(now)],
            )
        })?;
        Ok(())
    }

    /// The group's name, if an admin device set one.
    pub fn group_name(&self) -> Result<Option<String>, StoreError> {
        let name: Option<Option<String>> = self.db.with(|conn| {
            conn.query_row("SELECT name FROM group_settings WHERE id = 1", [], |row| {
                row.get(0)
            })
            .optional()
        })?;
        Ok(name.flatten())
    }
}
