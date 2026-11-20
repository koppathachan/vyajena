//! group-membership: Inactive devices are kept out; Automatic re-add. The
//! `presence` and `inactive_devices` tables (server-core tasks 8.1, 8.2).
//!
//! The messaging service decides when to mark a device inactive (a full
//! queue, or [`stale_members`] at a sweep) and does the queue side; this
//! module keeps the records: when each device was last connected, which
//! devices are inactive since when, the epoch of the server's latest external
//! Remove for each, and which inactive devices have come back.

use std::time::SystemTime;

use protocol::device_identity::DeviceId;
use rusqlite::{Connection, OptionalExtension, params};

use crate::runtime::clock::{from_unix_millis, unix_millis};
use crate::store::StoreError;
use crate::store::records;

/// An inactive device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inactive {
    /// The device.
    pub device_id: DeviceId,
    /// When the server marked it inactive.
    pub since: SystemTime,
    /// `RemovedNotice.missed_since`: the start of the period whose messages it
    /// will not receive.
    pub missed_since: SystemTime,
    /// When it first reconnected after being marked inactive.
    pub returned_at: Option<SystemTime>,
}

fn i64_of(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

fn device_id(bytes: &[u8]) -> Result<DeviceId, StoreError> {
    records::device_id_from(bytes)
        .ok_or_else(|| StoreError::Inconsistent("stored device id is not 16 bytes".into()))
}

/// Records that `device_id` had an open stream (or joined the group) at `now`.
pub fn touch(conn: &Connection, device_id: &DeviceId, now: SystemTime) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO presence (device_id, last_seen_at) VALUES (?1, ?2)
         ON CONFLICT (device_id) DO UPDATE SET last_seen_at = max(last_seen_at, ?2)",
        params![device_id.as_slice(), unix_millis(now)],
    )?;
    Ok(())
}

/// The inactive record of `device_id`, if it is inactive.
pub fn get(conn: &Connection, device_id: &DeviceId) -> Result<Option<Inactive>, StoreError> {
    conn.query_row(
        "SELECT device_id, since, missed_since, returned_at
         FROM inactive_devices WHERE device_id = ?1",
        params![device_id.as_slice()],
        row,
    )
    .optional()?
    .map(finish)
    .transpose()
}

type Row = (Vec<u8>, i64, i64, Option<i64>);

fn row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
}

fn finish((id, since, missed, returned): Row) -> Result<Inactive, StoreError> {
    Ok(Inactive {
        device_id: device_id(&id)?,
        since: from_unix_millis(since),
        missed_since: from_unix_millis(missed),
        returned_at: returned.map(from_unix_millis),
    })
}

/// Whether `device_id` is inactive.
pub fn is_inactive(conn: &Connection, device_id: &DeviceId) -> Result<bool, StoreError> {
    Ok(get(conn, device_id)?.is_some())
}

/// Marks `device_id` inactive at `now`, missing messages since
/// `missed_since`. Does nothing if it already is.
pub fn mark(
    conn: &Connection,
    device_id: &DeviceId,
    now: SystemTime,
    missed_since: SystemTime,
) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO inactive_devices (device_id, since, missed_since) VALUES (?1, ?2, ?3)
         ON CONFLICT (device_id) DO NOTHING",
        params![
            device_id.as_slice(),
            unix_millis(now),
            unix_millis(missed_since)
        ],
    )?;
    Ok(())
}

/// Records that the server proposed removing `device_id`'s leaf in `group_id` in
/// `epoch`.
pub fn set_proposed(
    conn: &Connection,
    device_id: &DeviceId,
    group_id: &[u8],
    epoch: u64,
) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO pi_removes (device_id, group_id, proposed_epoch) VALUES (?1, ?2, ?3)
         ON CONFLICT (device_id, group_id) DO UPDATE SET proposed_epoch = ?3",
        params![device_id.as_slice(), group_id, i64_of(epoch)],
    )?;
    Ok(())
}

/// Whether the server has a Remove pending for `device_id`'s leaf in `group_id`:
/// its leaf there is stale and a re-add must remove it.
pub fn remove_pending(
    conn: &Connection,
    device_id: &DeviceId,
    group_id: &[u8],
) -> Result<bool, StoreError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM pi_removes WHERE device_id = ?1 AND group_id = ?2)",
        params![device_id.as_slice(), group_id],
        |row| row.get(0),
    )?)
}

/// Records that inactive `device_id` reconnected at `now` (the first time
/// counts).
pub fn mark_returned(
    conn: &Connection,
    device_id: &DeviceId,
    now: SystemTime,
) -> Result<(), StoreError> {
    conn.execute(
        "UPDATE inactive_devices SET returned_at = coalesce(returned_at, ?2) WHERE device_id = ?1",
        params![device_id.as_slice(), unix_millis(now)],
    )?;
    Ok(())
}

/// A commit added `device_id` again: it is active from `now`.
pub fn clear(conn: &Connection, device_id: &DeviceId, now: SystemTime) -> Result<(), StoreError> {
    conn.execute(
        "DELETE FROM inactive_devices WHERE device_id = ?1",
        params![device_id.as_slice()],
    )?;
    touch(conn, device_id, now)
}

/// Devices with a leaf in any group that are not inactive, not in
/// `connected`, and have not been seen since `cutoff` (group-membership:
/// Inactive devices are kept out). A device never seen counts from its
/// registration.
pub fn stale_members(
    conn: &Connection,
    cutoff: SystemTime,
    connected: &[DeviceId],
) -> Result<Vec<DeviceId>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT d.device_id FROM devices d
         LEFT JOIN presence p ON p.device_id = d.device_id
         WHERE d.device_id IN (SELECT device_id FROM mls_members)
           AND d.device_id NOT IN (SELECT device_id FROM inactive_devices)
           AND coalesce(p.last_seen_at, d.created_at) <= ?1
         ORDER BY d.created_at, d.device_id",
    )?;
    let rows = stmt.query_map(params![unix_millis(cutoff)], |row| row.get::<_, Vec<u8>>(0))?;
    let mut stale = Vec::new();
    for id in rows {
        let id = device_id(&id?)?;
        if !connected.contains(&id) {
            stale.push(id);
        }
    }
    Ok(stale)
}

/// Inactive devices still in `group_id`'s tree whose latest server Remove there
/// is not for `epoch` (or that have none): the server reissues for them in each
/// new epoch.
pub fn needing_proposal(
    conn: &Connection,
    group_id: &[u8],
    epoch: u64,
) -> Result<Vec<DeviceId>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT i.device_id FROM inactive_devices i
         JOIN mls_members m ON m.device_id = i.device_id AND m.group_id = ?1
         LEFT JOIN pi_removes r ON r.device_id = i.device_id AND r.group_id = ?1
         WHERE r.proposed_epoch IS NULL OR r.proposed_epoch != ?2
         ORDER BY m.leaf_index",
    )?;
    let rows = stmt.query_map(params![group_id, i64_of(epoch)], |row| {
        row.get::<_, Vec<u8>>(0)
    })?;
    let mut ids = Vec::new();
    for id in rows {
        ids.push(device_id(&id?)?);
    }
    Ok(ids)
}

/// Inactive devices of non-revoked users that have reconnected and wait to be
/// re-added, with their usernames (group-membership: Automatic re-add).
pub fn waiting(conn: &Connection) -> Result<Vec<(DeviceId, String)>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT i.device_id, d.username FROM inactive_devices i
         JOIN devices d ON d.device_id = i.device_id
         WHERE i.returned_at IS NOT NULL AND d.revoked_at IS NULL
         ORDER BY i.returned_at, i.device_id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut ids = Vec::new();
    for r in rows {
        let (id, username) = r?;
        ids.push((device_id(&id)?, username));
    }
    Ok(ids)
}

/// Member devices of `group_id` that receive its envelopes and can re-add
/// another device: in the tree, not revoked, not inactive, with their
/// usernames.
pub fn active_members(
    conn: &Connection,
    group_id: &[u8],
) -> Result<Vec<(DeviceId, String)>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT m.device_id, d.username FROM mls_members m
         JOIN devices d ON d.device_id = m.device_id
         WHERE m.group_id = ?1
           AND d.revoked_at IS NULL
           AND m.device_id NOT IN (SELECT device_id FROM inactive_devices)
         ORDER BY m.leaf_index",
    )?;
    let rows = stmt.query_map(params![group_id], |row| {
        Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut ids = Vec::new();
    for r in rows {
        let (id, username) = r?;
        ids.push((device_id(&id)?, username));
    }
    Ok(ids)
}
