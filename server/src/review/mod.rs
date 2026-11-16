//! review-server: Review mode. Only on the store reviewers' server
//! (`review_mode = true`); the real server never runs any of this.
//!
//! - Each use of the bootstrap invite starts a fresh review: [`fresh_review`]
//!   empties every table except [`KEPT_TABLES`] (and keeps the bootstrap invite)
//!   in the signup's transaction, then the signup registers the new reviewer as
//!   the admin. The keys (TLS pin, MLS external sender) live in `key_dir`, not
//!   the database, so the invite keeps working.
//! - [`check_user_cap`]: the admin and at most one other user.

use std::time::SystemTime;

use protocol::device_identity::DeviceId;
use rusqlite::{Transaction, params};
use tonic::Status;

use crate::admin::invites;
use crate::store::StoreError;
use crate::store::records;

/// Tables a fresh review keeps: `meta` (the server id) and `invites` (only the
/// bootstrap row is kept). `sequence` keeps its one row, reset to 0. Every
/// other table, including ones added by later migrations, is emptied.
pub const KEPT_TABLES: &[&str] = &["meta", "invites", "sequence"];

/// Users a review server holds at most: the admin and one other.
pub const MAX_USERS: i64 = 2;

/// Empties the review server for a new reviewer, keeping the bootstrap invite
/// `bootstrap_code`, and returns the ids of the devices it deleted so their
/// open streams can be closed. Runs inside the caller's transaction: nothing
/// changes unless the caller commits.
pub fn fresh_review(
    tx: &Transaction<'_>,
    bootstrap_code: &str,
) -> Result<Vec<DeviceId>, StoreError> {
    let devices = {
        let mut stmt = tx.prepare("SELECT device_id FROM devices")?;
        let rows = stmt.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
        let mut devices = Vec::new();
        for row in rows {
            let bytes = row?;
            let id = records::device_id_from(&bytes)
                .ok_or_else(|| StoreError::Inconsistent("a device id is not 16 bytes".into()))?;
            devices.push(id);
        }
        devices
    };
    // Rows reference each other (devices -> users, deliveries -> envelopes
    // ...); check the foreign keys at commit, when all of them are gone.
    tx.execute_batch("PRAGMA defer_foreign_keys = ON")?;
    for table in wiped_tables(tx)? {
        tx.execute(&format!("DELETE FROM \"{table}\""), [])?;
    }
    tx.execute(
        "DELETE FROM invites WHERE code != ?1",
        params![bootstrap_code],
    )?;
    tx.execute("UPDATE sequence SET last_server_seq = 0", [])?;
    Ok(devices)
}

/// Every table a fresh review empties: all but [`KEPT_TABLES`] and SQLite's
/// own.
pub fn wiped_tables(tx: &Transaction<'_>) -> Result<Vec<String>, StoreError> {
    let mut stmt = tx.prepare(
        "SELECT name FROM sqlite_master
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut tables = Vec::new();
    for row in rows {
        let name = row?;
        if !KEPT_TABLES.contains(&name.as_str()) {
            tables.push(name);
        }
    }
    Ok(tables)
}

/// `Admin.CreateInvite` in review mode: refused with `FAILED_PRECONDITION` once
/// a user other than the admin exists or an invite is still pending.
pub fn check_user_cap(
    tx: &Transaction<'_>,
    admin_username: &str,
    now: SystemTime,
) -> Result<Result<(), Status>, StoreError> {
    let others: i64 = tx.query_row(
        "SELECT count(*) FROM users WHERE username != ?1 AND removed_at IS NULL",
        params![admin_username],
        |row| row.get(0),
    )?;
    let pending = invites::count_pending(tx, now)?;
    // The admin plus every other user and every pending invite's user.
    if 1 + others + pending >= MAX_USERS {
        return Ok(Err(Status::failed_precondition(
            "this review server holds the admin and one other user: remove that user \
             (or revoke the pending invite) first",
        )));
    }
    Ok(Ok(()))
}
