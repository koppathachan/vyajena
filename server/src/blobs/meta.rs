//! Blob metadata in the Pi's SQLite: which group a blob belongs to, who
//! uploaded it, when it dies and who acked it (blob-store: Pluggable storage
//! backend). The ciphertext is not here; the backend holds it.

use rusqlite::{Connection, OptionalExtension, params};

use crate::store::StoreError;

/// One `blobs` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobRow {
    /// The 16-byte blob id.
    pub blob_id: Vec<u8>,
    /// The group the blob was uploaded for.
    pub group_id: Vec<u8>,
    /// The device that uploaded it; it counts as having acked.
    pub uploader_device_id: Vec<u8>,
    /// Ciphertext size in bytes.
    pub size: i64,
    /// SHA-256 of the ciphertext.
    pub sha256: Vec<u8>,
    /// Upload time, Unix milliseconds.
    pub uploaded_at: i64,
    /// When it is deleted whatever its acks, Unix milliseconds.
    pub expires_at: i64,
}

impl BlobRow {
    /// Whether the blob has expired at `now_ms`; an expired blob is gone to
    /// every RPC even before the sweep deletes it.
    pub fn expired(&self, now_ms: i64) -> bool {
        self.expires_at <= now_ms
    }
}

/// The row for `blob_id`, expired or not.
pub fn find(conn: &Connection, blob_id: &[u8]) -> Result<Option<BlobRow>, StoreError> {
    Ok(conn
        .query_row(
            "SELECT blob_id, group_id, uploader_device_id, size, sha256, uploaded_at, expires_at
             FROM blobs WHERE blob_id = ?1",
            params![blob_id],
            |row| {
                Ok(BlobRow {
                    blob_id: row.get(0)?,
                    group_id: row.get(1)?,
                    uploader_device_id: row.get(2)?,
                    size: row.get(3)?,
                    sha256: row.get(4)?,
                    uploaded_at: row.get(5)?,
                    expires_at: row.get(6)?,
                })
            },
        )
        .optional()?)
}

/// Records a blob. Fails if its id is taken.
pub fn insert(conn: &Connection, row: &BlobRow) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO blobs
           (blob_id, group_id, uploader_device_id, size, sha256, uploaded_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            row.blob_id,
            row.group_id,
            row.uploader_device_id,
            row.size,
            row.sha256,
            row.uploaded_at,
            row.expires_at
        ],
    )?;
    Ok(())
}

/// Forgets a blob and its acks.
pub fn delete(conn: &Connection, blob_id: &[u8]) -> Result<(), StoreError> {
    conn.execute("DELETE FROM blobs WHERE blob_id = ?1", params![blob_id])?;
    Ok(())
}

/// Records that `device_id` no longer needs the blob. Idempotent.
pub fn add_ack(conn: &Connection, blob_id: &[u8], device_id: &[u8]) -> Result<(), StoreError> {
    conn.execute(
        "INSERT OR IGNORE INTO blob_acks (blob_id, device_id) VALUES (?1, ?2)",
        params![blob_id, device_id],
    )?;
    Ok(())
}

/// The devices that acked the blob.
pub fn acks(conn: &Connection, blob_id: &[u8]) -> Result<Vec<Vec<u8>>, StoreError> {
    let mut stmt = conn.prepare("SELECT device_id FROM blob_acks WHERE blob_id = ?1")?;
    let rows = stmt.query_map(params![blob_id], |row| row.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The ids of blobs that expired at or before `now_ms`.
pub fn expired_ids(conn: &Connection, now_ms: i64) -> Result<Vec<Vec<u8>>, StoreError> {
    let mut stmt =
        conn.prepare("SELECT blob_id FROM blobs WHERE expires_at <= ?1 ORDER BY expires_at")?;
    let rows = stmt.query_map(params![now_ms], |row| row.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}
