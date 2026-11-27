//! What the device keeps to build a local notification (change
//! `smarter-notifications`): the pending admin-events and the per-device
//! "Notification detail" level. Kinds and group ids only; never message text or a
//! sender.

use std::time::SystemTime;

use rusqlite::params;

use super::{Store, StoreError};
use crate::clock::unix_millis;
use crate::notification::{AdminEventKind, NotificationDetail};

impl Store {
    /// Records an admin-event on `group_id` until the app has notified for it
    /// ([`Store::clear_admin_events`]). One per kind and conversation: a repeat
    /// changes nothing.
    pub fn add_admin_event(
        &self,
        kind: AdminEventKind,
        group_id: &[u8],
        at: SystemTime,
    ) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO notification_admin_events (kind, group_id, at)
                 VALUES (?1, ?2, ?3) ON CONFLICT DO NOTHING",
                params![kind.as_str(), group_id, unix_millis(at)],
            )?;
            Ok(())
        })
    }

    /// The pending admin-events, oldest first: `(id, kind, group_id)`.
    pub fn admin_events(&self) -> Result<Vec<(i64, AdminEventKind, Vec<u8>)>, StoreError> {
        self.db.with(|conn| {
            let mut stmt = conn
                .prepare("SELECT id, kind, group_id FROM notification_admin_events ORDER BY id")?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            })?;
            let mut events = Vec::new();
            for row in rows {
                let (id, kind, group_id) = row?;
                if let Some(kind) = AdminEventKind::parse(&kind) {
                    events.push((id, kind, group_id));
                }
            }
            Ok(events)
        })
    }

    /// Drops every pending admin-event with an id up to `up_to` (the app notified
    /// for them, or the user is looking at the app).
    pub fn clear_admin_events(&self, up_to: i64) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "DELETE FROM notification_admin_events WHERE id <= ?1",
                params![up_to],
            )?;
            Ok(())
        })
    }

    /// The chosen "Notification detail" level; Basic until one is chosen.
    pub fn notification_detail(&self) -> Result<NotificationDetail, StoreError> {
        self.db.with(|conn| {
            let level: Option<String> = conn
                .query_row(
                    "SELECT level FROM notification_detail WHERE id = 1",
                    [],
                    |row| row.get(0),
                )
                .ok();
            Ok(level
                .as_deref()
                .and_then(NotificationDetail::parse)
                .unwrap_or_default())
        })
    }

    /// Chooses the "Notification detail" level for this device.
    pub fn set_notification_detail(&self, level: NotificationDetail) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO notification_detail (id, level) VALUES (1, ?1)
                 ON CONFLICT (id) DO UPDATE SET level = excluded.level",
                params![level.as_str()],
            )?;
            Ok(())
        })
    }
}
