//! group-membership: Automatic add of new users (flutter-app task 4.4).
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
      AND d.device_id NOT IN (SELECT device_id FROM mls_members)
      AND d.username NOT IN (
          SELECT d2.username FROM mls_members m
          JOIN devices d2 ON d2.device_id = m.device_id
          WHERE d2.revoked_at IS NULL)
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

/// The admin's member devices that can add a waiting device: in the tree,
/// not revoked, not inactive.
pub fn admin_helpers(conn: &Connection, admin: &str) -> Result<Vec<DeviceId>, StoreError> {
    Ok(super::inactivity::active_members(conn)?
        .into_iter()
        .filter(|(_, username)| username == admin)
        .map(|(device, _)| device)
        .collect())
}

/// What to announce after `device` uploaded KeyPackages: one `JoinNotice` per
/// admin member device, if `device` waits for its first add.
pub fn after_upload(
    conn: &Connection,
    device: &DeviceId,
    admin: &str,
    server_id: &str,
) -> Result<Vec<(DeviceId, pb::ServerFrame)>, StoreError> {
    let Some(group_id) = group_id(conn)? else {
        return Ok(Vec::new());
    };
    let Some(username) = is_waiting(conn, device)? else {
        return Ok(Vec::new());
    };
    let frame = notice(&group_id, device, &username, server_id);
    Ok(admin_helpers(conn, admin)?
        .into_iter()
        .map(|helper| (helper, frame.clone()))
        .collect())
}

fn group_id(conn: &Connection) -> Result<Option<Vec<u8>>, StoreError> {
    Ok(conn
        .query_row("SELECT group_id FROM mls_group WHERE id = 1", [], |row| {
            row.get(0)
        })
        .optional()?)
}
