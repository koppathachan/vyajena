//! Rows several services read: users and devices. Each service writes its own
//! tables; these readers are shared so the services need not call each other
//! (pi-server-core design D1).

use protocol::device_identity::DeviceId;
use rusqlite::{Connection, OptionalExtension, params};

use super::StoreError;

/// A stored user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserRow {
    /// The username (local part of the user id).
    pub username: String,
    /// The user identity public key (Ed25519, 32 bytes).
    pub identity_public_key: Vec<u8>,
    /// Whether `Admin.RemoveUser` removed the user; the username may then be
    /// invited again (group-membership: Removing a user).
    pub removed: bool,
}

/// A stored device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRow {
    /// The device id (16 bytes).
    pub device_id: DeviceId,
    /// Its user's username.
    pub username: String,
    /// The device auth public key.
    pub auth_public_key: Vec<u8>,
    /// The device's MLS signature public key, from its certificate.
    pub mls_signature_public_key: Vec<u8>,
    /// `SignedDeviceCertificate.certificate`, exactly as received.
    pub certificate: Vec<u8>,
    /// `SignedDeviceCertificate.signature`.
    pub certificate_signature: Vec<u8>,
    /// Whether the device is revoked.
    pub revoked: bool,
}

/// A 16-byte device id from a database or network value.
pub fn device_id_from(bytes: &[u8]) -> Option<DeviceId> {
    bytes.try_into().ok()
}

/// The user called `username`, if any.
pub fn user(conn: &Connection, username: &str) -> Result<Option<UserRow>, StoreError> {
    Ok(conn
        .query_row(
            "SELECT username, identity_public_key, removed_at IS NOT NULL
             FROM users WHERE username = ?1",
            params![username],
            |row| {
                Ok(UserRow {
                    username: row.get(0)?,
                    identity_public_key: row.get(1)?,
                    removed: row.get(2)?,
                })
            },
        )
        .optional()?)
}

const DEVICE_COLUMNS: &str = "device_id, username, auth_public_key, mls_signature_public_key,
     certificate, certificate_signature, revoked_at IS NOT NULL";

fn device_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(Vec<u8>, DeviceRow)> {
    let id: Vec<u8> = row.get(0)?;
    Ok((
        id,
        DeviceRow {
            device_id: [0; 16],
            username: row.get(1)?,
            auth_public_key: row.get(2)?,
            mls_signature_public_key: row.get(3)?,
            certificate: row.get(4)?,
            certificate_signature: row.get(5)?,
            revoked: row.get(6)?,
        },
    ))
}

fn with_id((id, mut row): (Vec<u8>, DeviceRow)) -> Result<DeviceRow, StoreError> {
    row.device_id = device_id_from(&id)
        .ok_or_else(|| StoreError::Inconsistent("stored device id is not 16 bytes".into()))?;
    Ok(row)
}

/// The device with id `device_id`, revoked or not.
pub fn device(conn: &Connection, device_id: &[u8]) -> Result<Option<DeviceRow>, StoreError> {
    let sql = format!("SELECT {DEVICE_COLUMNS} FROM devices WHERE device_id = ?1");
    conn.query_row(&sql, params![device_id], device_row)
        .optional()?
        .map(with_id)
        .transpose()
}

/// Every non-revoked device of `username`, oldest first.
pub fn active_devices(conn: &Connection, username: &str) -> Result<Vec<DeviceRow>, StoreError> {
    let sql = format!(
        "SELECT {DEVICE_COLUMNS} FROM devices
         WHERE username = ?1 AND revoked_at IS NULL ORDER BY created_at, device_id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![username], device_row)?;
    let mut devices = Vec::new();
    for row in rows {
        devices.push(with_id(row?)?);
    }
    Ok(devices)
}

/// The push token stored for `device_id` (device-identity: Push token
/// registration), for the push provider.
pub fn push_token(
    conn: &Connection,
    device_id: &DeviceId,
) -> Result<Option<crate::pb::PushToken>, StoreError> {
    let row: Option<(Option<i32>, Option<String>)> = conn
        .query_row(
            "SELECT push_platform, push_token FROM devices WHERE device_id = ?1",
            params![device_id.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    Ok(match row {
        Some((Some(platform), Some(token))) => Some(crate::pb::PushToken { platform, token }),
        _ => None,
    })
}

/// Clears `device_id`'s push token if it is still `token` (pi-runtime: Push
/// errors, "New token raced the error"). Returns whether it was cleared.
pub fn clear_push_token(
    conn: &Connection,
    device_id: &DeviceId,
    token: &str,
) -> Result<bool, StoreError> {
    let changed = conn.execute(
        "UPDATE devices SET push_platform = NULL, push_token = NULL
         WHERE device_id = ?1 AND push_token = ?2",
        params![device_id.as_slice(), token],
    )?;
    Ok(changed > 0)
}

/// One-time KeyPackages the Pi holds for `device_id`, for the low-KeyPackages
/// notice (key-directory: Replenishment signal).
pub fn one_time_key_packages(conn: &Connection, device_id: &DeviceId) -> Result<u32, StoreError> {
    Ok(conn.query_row(
        "SELECT count(*) FROM key_packages WHERE device_id = ?1 AND last_resort = 0",
        params![device_id.as_slice()],
        |row| row.get(0),
    )?)
}
