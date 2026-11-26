//! This device's local block list (moderation: Block a user, change
//! `report-and-block`).
//!
//! A set of blocked `UserId`s, kept only on this device: blocking writes no RPC
//! and no server marker and never travels to the user's other devices (design
//! D1). The display and notification paths read it to skip a blocked user's
//! messages; nothing is dropped, so unblocking restores them.

use rusqlite::params;

use super::{Store, StoreError};

impl Store {
    /// Adds `user_id` to the local block list. Idempotent.
    pub fn block_user(&self, user_id: &str) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO blocked_users (user_id) VALUES (?1) ON CONFLICT DO NOTHING",
                params![user_id],
            )?;
            Ok(())
        })
    }

    /// Removes `user_id` from the local block list. Idempotent.
    pub fn unblock_user(&self, user_id: &str) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "DELETE FROM blocked_users WHERE user_id = ?1",
                params![user_id],
            )?;
            Ok(())
        })
    }

    /// Whether `user_id` is on the local block list.
    pub fn is_user_blocked(&self, user_id: &str) -> Result<bool, StoreError> {
        self.db.with(|conn| {
            Ok(conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM blocked_users WHERE user_id = ?1)",
                params![user_id],
                |row| row.get(0),
            )?)
        })
    }

    /// The blocked `UserId`s, sorted.
    pub fn blocked_users(&self) -> Result<Vec<String>, StoreError> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare("SELECT user_id FROM blocked_users ORDER BY user_id")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            let mut users = Vec::new();
            for user_id in rows {
                users.push(user_id?);
            }
            Ok(users)
        })
    }
}
