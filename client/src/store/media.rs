//! The device media cache's index and the expired images (client-runtime:
//! Image download and cache; Expired media). The bytes are files in the media
//! directory; this is only what the core knows about them.

use std::time::SystemTime;

use rusqlite::{OptionalExtension, params};

use super::{Store, StoreError};
use crate::clock::{from_unix_millis, unix_millis};

/// One cached image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaEntry {
    /// The 16-byte blob id.
    pub blob_id: Vec<u8>,
    /// The WebP's size in bytes.
    pub size: u64,
    /// This device sent the image.
    pub own: bool,
    /// When it was cached.
    pub created_at: SystemTime,
    /// When it was last shown.
    pub last_used: SystemTime,
}

fn entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<MediaEntry> {
    Ok(MediaEntry {
        blob_id: row.get(0)?,
        size: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
        own: row.get(2)?,
        created_at: from_unix_millis(row.get(3)?),
        last_used: from_unix_millis(row.get(4)?),
    })
}

impl Store {
    /// Records (or replaces) a cached image.
    pub fn media_put(
        &self,
        blob_id: &[u8],
        size: u64,
        own: bool,
        now: SystemTime,
    ) -> Result<(), StoreError> {
        let size = i64::try_from(size)
            .map_err(|_| StoreError::Inconsistent("a cached image is over i64".into()))?;
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO media_cache (blob_id, size, own, created_at, last_used)
                 VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT (blob_id) DO UPDATE
                 SET size = excluded.size, own = own OR excluded.own,
                     last_used = excluded.last_used",
                params![blob_id, size, own, unix_millis(now)],
            )
        })?;
        Ok(())
    }

    /// The cached image `blob_id`, if there is one.
    pub fn media_entry(&self, blob_id: &[u8]) -> Result<Option<MediaEntry>, StoreError> {
        Ok(self.db.with(|conn| {
            conn.query_row(
                "SELECT blob_id, size, own, created_at, last_used
                 FROM media_cache WHERE blob_id = ?1",
                params![blob_id],
                entry,
            )
            .optional()
        })?)
    }

    /// `blob_id` was shown just now: it is the last to be evicted.
    pub fn media_touch(&self, blob_id: &[u8], now: SystemTime) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE media_cache SET last_used = ?2 WHERE blob_id = ?1",
                params![blob_id, unix_millis(now)],
            )
        })?;
        Ok(())
    }

    /// Forgets a cached image; returns whether there was one.
    pub fn media_remove(&self, blob_id: &[u8]) -> Result<bool, StoreError> {
        let removed = self.db.with(|conn| {
            conn.execute(
                "DELETE FROM media_cache WHERE blob_id = ?1",
                params![blob_id],
            )
        })?;
        Ok(removed > 0)
    }

    /// Every cached image, least recently shown first.
    pub fn media_entries_oldest_first(&self) -> Result<Vec<MediaEntry>, StoreError> {
        Ok(self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT blob_id, size, own, created_at, last_used
                 FROM media_cache ORDER BY last_used, created_at, blob_id",
            )?;
            let rows = stmt.query_map([], entry)?;
            rows.collect::<Result<Vec<_>, _>>()
        })?)
    }

    /// The Pi no longer has `blob_id`: its image message shows "media expired".
    pub fn mark_media_expired(&self, blob_id: &[u8], now: SystemTime) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT OR IGNORE INTO media_expired (blob_id, marked_at) VALUES (?1, ?2)",
                params![blob_id, unix_millis(now)],
            )
        })?;
        Ok(())
    }

    /// Whether `blob_id` is known to have expired on the Pi.
    pub fn is_media_expired(&self, blob_id: &[u8]) -> Result<bool, StoreError> {
        Ok(self
            .db
            .with(|conn| {
                conn.query_row(
                    "SELECT 1 FROM media_expired WHERE blob_id = ?1",
                    params![blob_id],
                    |_| Ok(()),
                )
                .optional()
            })?
            .is_some())
    }

    /// Marks own message `id` as waiting for its blob to be uploaded (or not).
    /// Set in the transaction that stores the message, so the outbox never
    /// sees an image message without it (client-runtime: Blob crypto and send
    /// order).
    pub fn set_blob_pending(
        &self,
        id: crate::events::MessageId,
        pending: bool,
    ) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE messages SET blob_pending = ?2 WHERE local_id = ?1",
                params![id.0, pending],
            )
        })?;
        Ok(())
    }

    /// Whether own message `id` still waits for its blob to be uploaded.
    pub fn is_blob_pending(&self, id: crate::events::MessageId) -> Result<bool, StoreError> {
        Ok(self
            .db
            .with(|conn| {
                conn.query_row(
                    "SELECT blob_pending FROM messages WHERE local_id = ?1",
                    params![id.0],
                    |row| row.get::<_, bool>(0),
                )
                .optional()
            })?
            .unwrap_or(false))
    }
}
