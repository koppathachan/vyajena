//! Presence: a user's manual online/offline flag (manual-presence, change
//! `manual-presence`).
//!
//! The flag is a single two-value marker per user, stored on the users row like
//! push tokens and the mute set. The server writes it only from a `SetPresence`
//! the user's own authenticated device sent (or the first-launch default at
//! `Devices.Register`, handled by the column default), and **never** derives it
//! from connection, heartbeats, reads or any activity (manual-presence: Manual
//! status, never inferred). It is routed, not content: the server stores the two
//! ids and the one flag only.
//!
//! On a change the server broadcasts a `PresenceUpdate` to every connected v4+
//! device of every **other** user who shares a conversation with the setter
//! (manual-presence: Broadcasting a status change, D1), and on each connect it
//! sends one snapshot `PresenceUpdate` per user the connecting user may see
//! (manual-presence: Status snapshot on connect).

use std::collections::BTreeSet;

use protocol::presence::Presence;
use rusqlite::{Connection, OptionalExtension as _, params};

use crate::pb;
use crate::runtime::clock::{to_timestamp, unix_millis};
use crate::store::StoreError;

/// A user's stored manual flag and when it was last set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StoredPresence {
    /// The wire `PresenceStatus` value (1 = ONLINE, 2 = OFFLINE).
    pub status: i32,
    /// `presence_updated_at` in ms since the Unix epoch (0 if never set).
    pub updated_at_ms: i64,
}

/// manual-presence: Setting your status. Records `status` against `username` and
/// stamps `updated_at` from the server clock. The caller ignores an
/// `UNSPECIFIED` status before calling this (the server never sets the flag from
/// it).
pub(crate) fn set(
    conn: &Connection,
    username: &str,
    status: Presence,
    now: std::time::SystemTime,
) -> Result<StoredPresence, StoreError> {
    // `updated_at` must strictly increase per user so a quick re-toggle (two
    // `SetPresence` within the same millisecond) is not dropped by the client's
    // latest-`updated_at`-wins rule (manual-presence: Latest value wins).
    let previous = get(conn, username)?.map_or(0, |p| p.updated_at_ms);
    let updated_at_ms = unix_millis(now).max(previous + 1);
    conn.execute(
        "UPDATE users SET manual_presence = ?2, presence_updated_at = ?3 WHERE username = ?1",
        params![username, status.wire_value(), updated_at_ms],
    )?;
    Ok(StoredPresence {
        status: status.wire_value(),
        updated_at_ms,
    })
}

/// The stored flag of `username`, or `None` for an unknown user.
pub(crate) fn get(conn: &Connection, username: &str) -> Result<Option<StoredPresence>, StoreError> {
    Ok(conn
        .query_row(
            "SELECT manual_presence, presence_updated_at FROM users WHERE username = ?1",
            params![username],
            |row| {
                Ok(StoredPresence {
                    status: row.get(0)?,
                    updated_at_ms: row.get(1)?,
                })
            },
        )
        .optional()?)
}

/// manual-presence: Broadcasting a status change (D1). Every **other** username
/// that shares an open conversation — the lobby, a named group or a DM — with
/// `username`, via any of either user's devices. A user who shares nothing with
/// `username` (or an unknown one) is not included.
pub(crate) fn co_member_usernames(
    conn: &Connection,
    username: &str,
) -> Result<BTreeSet<String>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT d2.username
           FROM mls_members m1
           JOIN devices d1 ON d1.device_id = m1.device_id
           JOIN mls_members m2 ON m2.group_id = m1.group_id
           JOIN devices d2 ON d2.device_id = m2.device_id
           JOIN mls_groups g ON g.group_id = m1.group_id
          WHERE d1.username = ?1 AND d2.username <> ?1 AND g.closed_at IS NULL",
    )?;
    let rows = stmt.query_map(params![username], |row| row.get::<_, String>(0))?;
    let mut users = BTreeSet::new();
    for user in rows {
        users.insert(user?);
    }
    Ok(users)
}

/// A `PresenceUpdate` server frame for `username`'s current flag.
pub(crate) fn update_frame(
    username: &str,
    server_id: &str,
    presence: StoredPresence,
) -> pb::ServerFrame {
    let updated_at = crate::runtime::clock::from_unix_millis(presence.updated_at_ms);
    pb::ServerFrame {
        frame: Some(pb::server_frame::Frame::PresenceUpdate(
            pb::PresenceUpdate {
                user: Some(pb::UserId {
                    username: username.to_owned(),
                    server_id: server_id.to_owned(),
                }),
                status: presence.status,
                updated_at: Some(to_timestamp(updated_at)),
            },
        )),
    }
}

/// manual-presence: Status snapshot on connect. One `PresenceUpdate` per user
/// `username` may see (the co-member set), carrying that user's current flag, so
/// the connecting device can render every visible avatar's dot at once. Only the
/// stream's caller gates this on protocol version 4+ (D7).
pub(crate) fn snapshot_frames(
    conn: &Connection,
    username: &str,
    server_id: &str,
) -> Result<Vec<pb::ServerFrame>, StoreError> {
    let mut frames = Vec::new();
    for other in co_member_usernames(conn, username)? {
        if let Some(presence) = get(conn, &other)? {
            frames.push(update_frame(&other, server_id, presence));
        }
    }
    Ok(frames)
}
