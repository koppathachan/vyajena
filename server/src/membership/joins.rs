//! group-membership: Automatic add of new users (flutter-app task 4.4), and
//! direct-messages: DM members kept complete (change `dms`, design D6).
//!
//! Which devices wait for their first add, and which admin devices hear
//! about them. A device waits when it is not revoked, has never been a group
//! member, its user is not removed and has no (non-revoked) member device,
//! and it has uploaded KeyPackages (so an admin device can fetch one). The Pi
//! tells the
//! admin's connected member devices with a `JoinNotice` when such a device
//! uploads its KeyPackages (right after `Devices.Register` in the signup) and
//! each time an admin device connects; the admin's device does the identity
//! checks and the add.

use protocol::device_identity::DeviceId;
use rusqlite::{Connection, OptionalExtension, params};

use crate::pb;
use crate::store::{StoreError, records};

const WAITING: &str = "
    SELECT d.device_id, d.username FROM devices d
    JOIN users u ON u.username = d.username
    WHERE d.revoked_at IS NULL
      AND d.was_member = 0
      AND u.removed_at IS NULL
      AND d.device_id NOT IN (
          SELECT m.device_id FROM mls_members m
          JOIN mls_groups g ON g.group_id = m.group_id WHERE g.kind = 'main')
      AND d.username NOT IN (
          SELECT d2.username FROM mls_members m
          JOIN mls_groups g ON g.group_id = m.group_id
          JOIN devices d2 ON d2.device_id = m.device_id
          WHERE g.kind = 'main' AND d2.revoked_at IS NULL)
      AND EXISTS (SELECT 1 FROM key_packages k WHERE k.device_id = d.device_id)";

fn device_id(bytes: &[u8]) -> Result<DeviceId, StoreError> {
    records::device_id_from(bytes)
        .ok_or_else(|| StoreError::Inconsistent("stored device id is not 16 bytes".into()))
}

/// The devices waiting for their first add, oldest first, with their
/// usernames.
pub fn waiting(conn: &Connection) -> Result<Vec<(DeviceId, String)>, StoreError> {
    let mut stmt = conn.prepare(&format!("{WAITING} ORDER BY d.created_at, d.device_id"))?;
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

/// Whether `device` waits for its first add; its username if so.
pub fn is_waiting(conn: &Connection, device: &DeviceId) -> Result<Option<String>, StoreError> {
    let mut stmt = conn.prepare(&format!("{WAITING} AND d.device_id = ?1"))?;
    let mut rows = stmt.query(params![device.as_slice()])?;
    match rows.next()? {
        Some(row) => Ok(Some(row.get(1)?)),
        None => Ok(None),
    }
}

/// The `JoinNotice` frame for `device` of `username`.
pub fn notice(
    group_id: &[u8],
    device: &DeviceId,
    username: &str,
    server_id: &str,
) -> pb::ServerFrame {
    pb::ServerFrame {
        frame: Some(pb::server_frame::Frame::JoinNotice(pb::JoinNotice {
            group_id: group_id.to_vec(),
            device: Some(pb::DeviceRef {
                user_id: Some(pb::UserId {
                    username: username.to_owned(),
                    server_id: server_id.to_owned(),
                }),
                device_id: device.to_vec(),
            }),
        })),
    }
}

/// The admin's member devices that can add a waiting device: in the main
/// group's tree, not revoked, not inactive.
pub fn admin_helpers(conn: &Connection, admin: &str) -> Result<Vec<DeviceId>, StoreError> {
    let Some(group_id) = group_id(conn)? else {
        return Ok(Vec::new());
    };
    Ok(super::inactivity::active_members(conn, &group_id)?
        .into_iter()
        .filter(|(_, username)| username == admin)
        .map(|(device, _)| device)
        .collect())
}

/// What to announce after `device` uploaded KeyPackages: one `JoinNotice` per
/// admin member device, if `device` waits for its first add, and the DM
/// notices about it (direct-messages: DM members kept complete).
pub fn after_upload(
    conn: &Connection,
    device: &DeviceId,
    admin: &str,
    server_id: &str,
) -> Result<Vec<(DeviceId, pb::ServerFrame)>, StoreError> {
    let mut out = dm_notices_about(conn, device, server_id)?;
    let Some(group_id) = group_id(conn)? else {
        return Ok(out);
    };
    let Some(username) = is_waiting(conn, device)? else {
        return Ok(out);
    };
    let frame = notice(&group_id, device, &username, server_id);
    out.extend(
        admin_helpers(conn, admin)?
            .into_iter()
            .map(|helper| (helper, frame.clone())),
    );
    Ok(out)
}

fn group_id(conn: &Connection) -> Result<Option<Vec<u8>>, StoreError> {
    Ok(conn
        .query_row(
            "SELECT group_id FROM mls_groups WHERE kind = 'main'",
            [],
            |row| row.get(0),
        )
        .optional()?)
}

/// direct-messages: DM members kept complete. The devices `dm_group_id` (an
/// open DM between `users`) still needs: non-revoked, active, DM-capable
/// devices of either user (their last-resort KeyPackage lists `0xF0A2`),
/// already in the main group (so a core always has the main group before a
/// DM), that have no leaf in it, or whose leaf there waits for the Pi's
/// Remove.
pub fn dm_needing(
    conn: &Connection,
    dm_group_id: &[u8],
    users: &(String, String),
) -> Result<Vec<(DeviceId, String)>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT d.device_id, d.username FROM devices d
         JOIN users u ON u.username = d.username
         WHERE d.username IN (?2, ?3)
           AND d.revoked_at IS NULL
           AND u.removed_at IS NULL
           AND d.device_id NOT IN (SELECT device_id FROM inactive_devices)
           AND d.device_id IN (SELECT m.device_id FROM mls_members m
                               JOIN mls_groups g ON g.group_id = m.group_id
                               WHERE g.kind = 'main')
           AND EXISTS (SELECT 1 FROM key_packages k
                       WHERE k.device_id = d.device_id AND k.last_resort = 1
                         AND k.dm_capable = 1)
           AND (d.device_id NOT IN (SELECT device_id FROM mls_members WHERE group_id = ?1)
                OR d.device_id IN (SELECT device_id FROM pi_removes WHERE group_id = ?1))
         ORDER BY d.created_at, d.device_id",
    )?;
    let rows = stmt.query_map(params![dm_group_id, users.0, users.1], |row| {
        Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut ids = Vec::new();
    for r in rows {
        let (id, username) = r?;
        ids.push((device_id(&id)?, username));
    }
    Ok(ids)
}

/// The DM's member devices that can add a device it needs: active members
/// that are not themselves waiting for a re-add there.
pub fn dm_helpers(
    conn: &Connection,
    dm_group_id: &[u8],
    needing: &[(DeviceId, String)],
) -> Result<Vec<DeviceId>, StoreError> {
    Ok(super::inactivity::active_members(conn, dm_group_id)?
        .into_iter()
        .map(|(device, _)| device)
        .filter(|device| !needing.iter().any(|(n, _)| n == device))
        .collect())
}

/// Every `JoinNotice` an open DM owes right now: one per (helper, needed
/// device).
pub fn dm_notices(
    conn: &Connection,
    dm: &super::group::GroupRow,
    server_id: &str,
) -> Result<Vec<(DeviceId, pb::ServerFrame)>, StoreError> {
    let super::group::GroupKind::Dm {
        users,
        closed_at: None,
    } = &dm.kind
    else {
        return Ok(Vec::new());
    };
    let needing = dm_needing(conn, &dm.group_id, users)?;
    if needing.is_empty() {
        return Ok(Vec::new());
    }
    let helpers = dm_helpers(conn, &dm.group_id, &needing)?;
    let mut out = Vec::new();
    for helper in helpers {
        for (device, username) in &needing {
            out.push((helper, notice(&dm.group_id, device, username, server_id)));
        }
    }
    Ok(out)
}

/// The DM notices about `device`: for each open DM of its user that needs it,
/// one `JoinNotice` per helper.
pub fn dm_notices_about(
    conn: &Connection,
    device: &DeviceId,
    server_id: &str,
) -> Result<Vec<(DeviceId, pb::ServerFrame)>, StoreError> {
    let mut out = Vec::new();
    for dm in super::group::open_dms(conn)? {
        out.extend(
            dm_notices(conn, &dm, server_id)?
                .into_iter()
                .filter(|(_, frame)| notice_is_about(frame, device)),
        );
    }
    Ok(out)
}

/// The DM notices `helper` should hear now: for each open DM it is an active
/// member of, one per device the DM needs.
pub fn dm_notices_for(
    conn: &Connection,
    helper: &DeviceId,
    server_id: &str,
) -> Result<Vec<pb::ServerFrame>, StoreError> {
    let mut out = Vec::new();
    for dm in super::group::open_dms(conn)? {
        out.extend(
            dm_notices(conn, &dm, server_id)?
                .into_iter()
                .filter(|(to, _)| to == helper)
                .map(|(_, frame)| frame),
        );
    }
    Ok(out)
}

fn notice_is_about(frame: &pb::ServerFrame, device: &DeviceId) -> bool {
    matches!(
        &frame.frame,
        Some(pb::server_frame::Frame::JoinNotice(n))
            if n.device.as_ref().is_some_and(|d| d.device_id.as_slice() == device.as_slice())
    )
}
