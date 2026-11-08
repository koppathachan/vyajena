//! device-identity: Invites. Invite rows: single use, bound to a username,
//! expiring [`INVITE_TTL`] after creation.

use std::time::SystemTime;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use protocol::device_identity::INVITE_TTL;
use rusqlite::{OptionalExtension, Transaction, params};

use crate::runtime::clock::{from_unix_millis, unix_millis};
use crate::store::StoreError;

/// Random bytes in an invite code (128 bits).
pub const INVITE_CODE_BYTES: usize = 16;

/// A stored invite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    /// The single-use code, `RegisterRequest.invite_code`.
    pub code: String,
    /// The username the invite creates; the user id is `<username>@<server_id>`.
    pub username: String,
    /// When it was created.
    pub created_at: SystemTime,
    /// When it stops being usable, if unused.
    pub expires_at: SystemTime,
    /// Whether it is the bootstrap invite for the admin.
    pub bootstrap: bool,
}

/// A fresh invite code: 16 random bytes, base64url without padding (22 chars).
pub fn new_invite_code() -> Result<String, StoreError> {
    let mut bytes = [0u8; INVITE_CODE_BYTES];
    getrandom::fill(&mut bytes)
        .map_err(|e| StoreError::Inconsistent(format!("no randomness for invite code: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// Creates and stores a new invite for `username`, created at `now`.
pub fn create(
    tx: &Transaction<'_>,
    username: &str,
    now: SystemTime,
    bootstrap: bool,
) -> Result<Invite, StoreError> {
    let invite = Invite {
        code: new_invite_code()?,
        username: username.to_owned(),
        created_at: now,
        expires_at: now + INVITE_TTL,
        bootstrap,
    };
    tx.execute(
        "INSERT INTO invites (code, username, created_at, expires_at, bootstrap)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            invite.code,
            invite.username,
            unix_millis(invite.created_at),
            unix_millis(invite.expires_at),
            invite.bootstrap,
        ],
    )?;
    Ok(invite)
}

/// The newest bootstrap invite that is unused, unrevoked and unexpired at `now`.
pub fn usable_bootstrap(
    tx: &Transaction<'_>,
    now: SystemTime,
) -> Result<Option<Invite>, StoreError> {
    let row = tx
        .query_row(
            "SELECT code, username, created_at, expires_at FROM invites
             WHERE bootstrap = 1 AND used_at IS NULL AND revoked_at IS NULL
               AND expires_at > ?1
             ORDER BY created_at DESC LIMIT 1",
            params![unix_millis(now)],
            |row| {
                Ok(Invite {
                    code: row.get(0)?,
                    username: row.get(1)?,
                    created_at: from_unix_millis(row.get(2)?),
                    expires_at: from_unix_millis(row.get(3)?),
                    bootstrap: true,
                })
            },
        )
        .optional()?;
    Ok(row)
}

/// Every invite that is unused, unrevoked and unexpired at `now`, oldest first
/// (device-identity: Invites).
pub fn list_unused(tx: &Transaction<'_>, now: SystemTime) -> Result<Vec<Invite>, StoreError> {
    let mut stmt = tx.prepare(
        "SELECT code, username, created_at, expires_at, bootstrap FROM invites
         WHERE used_at IS NULL AND revoked_at IS NULL AND expires_at > ?1
         ORDER BY created_at, code",
    )?;
    let rows = stmt.query_map(params![unix_millis(now)], |row| {
        Ok(Invite {
            code: row.get(0)?,
            username: row.get(1)?,
            created_at: from_unix_millis(row.get(2)?),
            expires_at: from_unix_millis(row.get(3)?),
            bootstrap: row.get(4)?,
        })
    })?;
    let mut invites = Vec::new();
    for row in rows {
        invites.push(row?);
    }
    Ok(invites)
}

/// What [`revoke`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevokeOutcome {
    /// The invite is now revoked (or already was).
    Revoked,
    /// No invite has that code.
    NotFound,
    /// The invite was already used; it cannot be revoked.
    AlreadyUsed,
}

/// Revokes the unused invite `code` at `now`.
pub fn revoke(
    tx: &Transaction<'_>,
    code: &str,
    now: SystemTime,
) -> Result<RevokeOutcome, StoreError> {
    let used: Option<Option<i64>> = tx
        .query_row(
            "SELECT used_at FROM invites WHERE code = ?1",
            params![code],
            |row| row.get(0),
        )
        .optional()?;
    match used {
        None => Ok(RevokeOutcome::NotFound),
        Some(Some(_)) => Ok(RevokeOutcome::AlreadyUsed),
        Some(None) => {
            tx.execute(
                "UPDATE invites SET revoked_at = coalesce(revoked_at, ?2) WHERE code = ?1",
                params![code, unix_millis(now)],
            )?;
            Ok(RevokeOutcome::Revoked)
        }
    }
}
