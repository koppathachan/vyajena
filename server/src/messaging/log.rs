//! The `sequence`, `envelopes`, `deliveries`, `cursors` and `dedup` tables
//! (message-delivery: Per-device queues; Commit log; Acknowledgement;
//! Idempotent sends; server-core design D4).
//!
//! One `envelopes` row per accepted envelope; one `deliveries` row per device
//! that still has to receive it. A device's application queue is its
//! deliveries of `APPLICATION` envelopes; its commit-log entries are its
//! deliveries of `COMMIT` and `PROPOSAL` envelopes (as a `WELCOME` for the
//! devices a commit adds), and its cursor is the highest `server_seq` it
//! acknowledged. Delivered in `server_seq` order, they make the merged
//! catch-up the spec asks for.

use std::time::SystemTime;

use protocol::device_identity::DeviceId;
use rusqlite::{Connection, OptionalExtension, params};

use crate::pb;
use crate::pb::envelope::Kind;
use crate::runtime::clock::{from_unix_millis, to_timestamp, unix_millis};
use crate::store::StoreError;

pub(crate) fn i64_of(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

pub(crate) fn u64_of(n: i64) -> u64 {
    u64::try_from(n).unwrap_or(0)
}

/// The main group's id and current epoch, if it exists.
pub(crate) fn group_row(conn: &Connection) -> Result<Option<(Vec<u8>, u64)>, StoreError> {
    Ok(crate::membership::group::main_row(conn)?.map(|row| (row.group_id, row.epoch)))
}

/// Hands out the next `server_seq`.
pub(crate) fn next_seq(conn: &Connection) -> Result<u64, StoreError> {
    let seq: i64 = conn.query_row(
        "UPDATE sequence SET last_server_seq = last_server_seq + 1 WHERE id = 1
         RETURNING last_server_seq",
        [],
        |row| row.get(0),
    )?;
    Ok(u64_of(seq))
}

/// A row for `envelopes`.
pub(crate) struct NewEnvelope<'a> {
    pub server_seq: u64,
    pub group_id: &'a [u8],
    pub msg_id: &'a [u8],
    pub sender: Option<&'a DeviceId>,
    pub kind: Kind,
    pub epoch: u64,
    pub ciphertext: &'a [u8],
    pub welcome: Option<&'a [u8]>,
    pub accepted_at: SystemTime,
}

pub(crate) fn insert_envelope(conn: &Connection, e: &NewEnvelope<'_>) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO envelopes (server_seq, msg_id, sender_device_id, kind, epoch, ciphertext,
                                welcome, accepted_at, group_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            i64_of(e.server_seq),
            e.msg_id,
            e.sender.map(|d| d.as_slice()),
            e.kind as i32,
            i64_of(e.epoch),
            e.ciphertext,
            e.welcome,
            unix_millis(e.accepted_at),
            e.group_id,
        ],
    )?;
    Ok(())
}

/// Queues envelope `server_seq` for `device_id`, as a `WELCOME` if `welcome`.
pub(crate) fn insert_delivery(
    conn: &Connection,
    device_id: &DeviceId,
    server_seq: u64,
    welcome: bool,
) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO deliveries (device_id, server_seq, welcome) VALUES (?1, ?2, ?3)",
        params![device_id.as_slice(), i64_of(server_seq), welcome],
    )?;
    Ok(())
}

/// Application entries queued for `device_id`, all groups together
/// (message-delivery: Per-device queues, capped at `QUEUE_CAP`;
/// direct-messages: Inactivity across groups).
pub(crate) fn application_queue_len(
    conn: &Connection,
    device_id: &DeviceId,
) -> Result<usize, StoreError> {
    let n: i64 = conn.query_row(
        "SELECT count(*) FROM deliveries d JOIN envelopes e ON e.server_seq = d.server_seq
         WHERE d.device_id = ?1 AND e.kind = ?2",
        params![device_id.as_slice(), Kind::Application as i32],
        |row| row.get(0),
    )?;
    Ok(usize::try_from(n).unwrap_or(usize::MAX))
}

/// Deletes everything queued for `device_id`, in every group, and returns the
/// `accepted_at` of the oldest application entry among it.
pub(crate) fn delete_all_deliveries(
    conn: &Connection,
    device_id: &DeviceId,
) -> Result<Option<SystemTime>, StoreError> {
    let oldest: Option<i64> = conn.query_row(
        "SELECT min(e.accepted_at) FROM deliveries d
         JOIN envelopes e ON e.server_seq = d.server_seq
         WHERE d.device_id = ?1 AND e.kind = ?2",
        params![device_id.as_slice(), Kind::Application as i32],
        |row| row.get(0),
    )?;
    conn.execute(
        "DELETE FROM deliveries WHERE device_id = ?1",
        params![device_id.as_slice()],
    )?;
    Ok(oldest.map(from_unix_millis))
}

/// The cursor of `device_id`, created at 0 if it has none.
pub(crate) fn cursor(conn: &Connection, device_id: &DeviceId) -> Result<u64, StoreError> {
    conn.execute(
        "INSERT INTO cursors (device_id, acked_seq) VALUES (?1, 0)
         ON CONFLICT (device_id) DO NOTHING",
        params![device_id.as_slice()],
    )?;
    let acked: i64 = conn.query_row(
        "SELECT acked_seq FROM cursors WHERE device_id = ?1",
        params![device_id.as_slice()],
        |row| row.get(0),
    )?;
    Ok(u64_of(acked))
}

/// message-delivery: Acknowledgement. Deletes `device_id`'s entries up to
/// `server_seq`, advances its cursor, and drops envelopes nobody needs now.
///
/// One sequence serves every group and a device receives and processes its
/// entries in that one order, so an `Ack` in any group covers its entries
/// of every group up to `server_seq` (change `dms`, design D13).
pub(crate) fn ack(
    conn: &Connection,
    device_id: &DeviceId,
    server_seq: u64,
) -> Result<(), StoreError> {
    let seq = i64_of(server_seq);
    conn.execute(
        "INSERT INTO cursors (device_id, acked_seq) VALUES (?1, ?2)
         ON CONFLICT (device_id) DO UPDATE SET acked_seq = max(acked_seq, ?2)",
        params![device_id.as_slice(), seq],
    )?;
    conn.execute(
        "DELETE FROM deliveries WHERE device_id = ?1 AND server_seq <= ?2",
        params![device_id.as_slice(), seq],
    )?;
    conn.execute(
        "DELETE FROM envelopes WHERE server_seq <= ?1
           AND NOT EXISTS (SELECT 1 FROM deliveries d WHERE d.server_seq = envelopes.server_seq)",
        params![seq],
    )?;
    Ok(())
}

/// Deletes envelopes no device still has to receive.
pub(crate) fn collect_garbage(conn: &Connection) -> Result<(), StoreError> {
    conn.execute(
        "DELETE FROM envelopes
         WHERE NOT EXISTS (SELECT 1 FROM deliveries d WHERE d.server_seq = envelopes.server_seq)",
        [],
    )?;
    Ok(())
}

/// Up to `limit` entries of `device_id` after `after` and below `below`, in
/// `server_seq` order, as the envelopes the device receives
/// (message-delivery: Ordered delivery and catch-up). A commit's Welcome goes
/// out as a `WELCOME` envelope at the commit's `server_seq` with the epoch the
/// commit creates.
pub(crate) fn pending(
    conn: &Connection,
    server_id: &str,
    device_id: &DeviceId,
    after: u64,
    below: u64,
    limit: usize,
) -> Result<Vec<(u64, pb::Envelope)>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT e.server_seq, e.msg_id, e.sender_device_id, s.username, e.kind, e.epoch,
                e.ciphertext, e.welcome, e.accepted_at, d.welcome, e.group_id
         FROM deliveries d
         JOIN envelopes e ON e.server_seq = d.server_seq
         LEFT JOIN devices s ON s.device_id = e.sender_device_id
         WHERE d.device_id = ?1 AND d.server_seq > ?2 AND d.server_seq < ?4
         ORDER BY d.server_seq LIMIT ?3",
    )?;
    let rows = stmt.query_map(
        params![
            device_id.as_slice(),
            i64_of(after),
            i64::try_from(limit).unwrap_or(i64::MAX),
            i64_of(below)
        ],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Option<Vec<u8>>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i32>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, Vec<u8>>(6)?,
                row.get::<_, Option<Vec<u8>>>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, bool>(9)?,
                row.get::<_, Vec<u8>>(10)?,
            ))
        },
    )?;
    let mut out = Vec::new();
    for row in rows {
        let (
            seq,
            msg_id,
            sender_id,
            sender_name,
            kind,
            epoch,
            ciphertext,
            welcome,
            at,
            as_welcome,
            group_id,
        ) = row?;
        let sender = match (sender_id, sender_name) {
            (Some(device_id), Some(username)) => Some(pb::DeviceRef {
                user_id: Some(pb::UserId {
                    username,
                    server_id: server_id.to_owned(),
                }),
                device_id,
            }),
            _ => None,
        };
        let seq = u64_of(seq);
        let (kind, epoch, ciphertext) = if as_welcome {
            let welcome = welcome.ok_or_else(|| {
                StoreError::Inconsistent("a Welcome delivery without a Welcome".into())
            })?;
            (Kind::Welcome as i32, u64_of(epoch) + 1, welcome)
        } else {
            (kind, u64_of(epoch), ciphertext)
        };
        out.push((
            seq,
            pb::Envelope {
                msg_id,
                group_id,
                epoch,
                kind,
                ciphertext,
                welcome: None,
                sender,
                server_seq: seq,
                accepted_at: Some(to_timestamp(from_unix_millis(at))),
            },
        ));
    }
    Ok(out)
}

/// An earlier accepted envelope with the same (sender, `msg_id`).
pub(crate) struct DedupRow {
    pub server_seq: u64,
    pub accepted_at: SystemTime,
    pub ciphertext_sha256: Vec<u8>,
    pub welcome_sha256: Option<Vec<u8>>,
}

pub(crate) fn dedup_lookup(
    conn: &Connection,
    sender: &DeviceId,
    msg_id: &[u8],
) -> Result<Option<DedupRow>, StoreError> {
    Ok(conn
        .query_row(
            "SELECT server_seq, accepted_at, ciphertext_sha256, welcome_sha256 FROM dedup
             WHERE sender_device_id = ?1 AND msg_id = ?2",
            params![sender.as_slice(), msg_id],
            |row| {
                Ok(DedupRow {
                    server_seq: u64_of(row.get(0)?),
                    accepted_at: from_unix_millis(row.get(1)?),
                    ciphertext_sha256: row.get(2)?,
                    welcome_sha256: row.get(3)?,
                })
            },
        )
        .optional()?)
}

pub(crate) fn dedup_insert(
    conn: &Connection,
    sender: &DeviceId,
    msg_id: &[u8],
    row: &DedupRow,
) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO dedup (sender_device_id, msg_id, server_seq, accepted_at, ciphertext_sha256,
                            welcome_sha256)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            sender.as_slice(),
            msg_id,
            i64_of(row.server_seq),
            unix_millis(row.accepted_at),
            row.ciphertext_sha256,
            row.welcome_sha256,
        ],
    )?;
    Ok(())
}

/// Drops dedup records accepted before `cutoff`.
pub(crate) fn dedup_prune(conn: &Connection, cutoff: SystemTime) -> Result<(), StoreError> {
    conn.execute(
        "DELETE FROM dedup WHERE accepted_at < ?1",
        params![unix_millis(cutoff)],
    )?;
    Ok(())
}

/// message-delivery: Expiry and gap notice. Deletes application entries
/// accepted at or before `cutoff` and records, per device and group, the
/// newest one that expired. Returns the devices concerned.
pub(crate) fn expire(conn: &Connection, cutoff: SystemTime) -> Result<Vec<DeviceId>, StoreError> {
    let cutoff = unix_millis(cutoff);
    let mut stmt = conn.prepare(
        "SELECT d.device_id, e.group_id, max(e.accepted_at) FROM deliveries d
         JOIN envelopes e ON e.server_seq = d.server_seq
         WHERE e.kind = ?1 AND e.accepted_at <= ?2
         GROUP BY d.device_id, e.group_id",
    )?;
    let rows = stmt.query_map(params![Kind::Application as i32, cutoff], |row| {
        Ok((
            row.get::<_, Vec<u8>>(0)?,
            row.get::<_, Vec<u8>>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;
    let mut expired = Vec::new();
    for row in rows {
        let (id, group_id, newest) = row?;
        conn.execute(
            "INSERT INTO gaps (device_id, group_id, newest_expired_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (device_id, group_id) DO UPDATE
             SET newest_expired_at = max(newest_expired_at, ?3)",
            params![id, group_id, newest],
        )?;
        let id = crate::store::records::device_id_from(&id)
            .ok_or_else(|| StoreError::Inconsistent("stored device id is not 16 bytes".into()))?;
        if !expired.contains(&id) {
            expired.push(id);
        }
    }
    conn.execute(
        "DELETE FROM deliveries WHERE server_seq IN
             (SELECT server_seq FROM envelopes WHERE kind = ?1 AND accepted_at <= ?2)",
        params![Kind::Application as i32, cutoff],
    )?;
    Ok(expired)
}

/// The unreported newest expired entry of `device_id` in each group, if any.
pub(crate) fn gaps(
    conn: &Connection,
    device_id: &DeviceId,
) -> Result<Vec<(Vec<u8>, SystemTime)>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT group_id, newest_expired_at FROM gaps WHERE device_id = ?1 ORDER BY group_id",
    )?;
    let rows = stmt.query_map(params![device_id.as_slice()], |row| {
        Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut gaps = Vec::new();
    for row in rows {
        let (group_id, at) = row?;
        gaps.push((group_id, from_unix_millis(at)));
    }
    Ok(gaps)
}

/// The gap notice for `device_id` in `group_id` was sent.
pub(crate) fn clear_gap(
    conn: &Connection,
    device_id: &DeviceId,
    group_id: &[u8],
) -> Result<(), StoreError> {
    conn.execute(
        "DELETE FROM gaps WHERE device_id = ?1 AND group_id = ?2",
        params![device_id.as_slice(), group_id],
    )?;
    Ok(())
}
