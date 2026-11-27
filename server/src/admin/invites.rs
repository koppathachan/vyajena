//! device-identity: Invites. Invite rows: single use, bound to a username,
//! expiring [`INVITE_TTL`] after creation. The one exception is the store
//! reviewers' server's reusable admin invite (device-identity: Reusable review
//! admin invite): a row with `reusable = 1` that never expires and is never
//! used up. Every single-use query here skips it.

use std::time::{Duration, SystemTime};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
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
    invite_ttl: Duration,
    bootstrap: bool,
) -> Result<Invite, StoreError> {
    let invite = Invite {
        code: new_invite_code()?,
        username: username.to_owned(),
        created_at: now,
        expires_at: now + invite_ttl,
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
             WHERE bootstrap = 1 AND reusable = 0 AND used_at IS NULL
               AND revoked_at IS NULL AND expires_at > ?1
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

/// The newest non-bootstrap invite for `username` that is unused, unrevoked and
/// unexpired at `now` (device-identity: At most one pending invite per username).
/// Used to dedupe `Admin.CreateInvite` so asking twice returns the same invite.
pub fn usable_for_username(
    tx: &Transaction<'_>,
    username: &str,
    now: SystemTime,
) -> Result<Option<Invite>, StoreError> {
    let row = tx
        .query_row(
            "SELECT code, username, created_at, expires_at FROM invites
             WHERE username = ?1 AND bootstrap = 0 AND reusable = 0 AND used_at IS NULL
               AND revoked_at IS NULL AND expires_at > ?2
             ORDER BY created_at DESC LIMIT 1",
            params![username, unix_millis(now)],
            |row| {
                Ok(Invite {
                    code: row.get(0)?,
                    username: row.get(1)?,
                    created_at: from_unix_millis(row.get(2)?),
                    expires_at: from_unix_millis(row.get(3)?),
                    bootstrap: false,
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
         WHERE reusable = 0 AND used_at IS NULL AND revoked_at IS NULL AND expires_at > ?1
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevokeOutcome {
    /// The invite for this username is now revoked (or already was).
    Revoked(String),
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
    let used: Option<(Option<i64>, String)> = tx
        .query_row(
            "SELECT used_at, username FROM invites WHERE code = ?1 AND reusable = 0",
            params![code],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    match used {
        None => Ok(RevokeOutcome::NotFound),
        Some((Some(_), _)) => Ok(RevokeOutcome::AlreadyUsed),
        Some((None, username)) => {
            tx.execute(
                "UPDATE invites SET revoked_at = coalesce(revoked_at, ?2) WHERE code = ?1",
                params![code, unix_millis(now)],
            )?;
            Ok(RevokeOutcome::Revoked(username))
        }
    }
}

/// Revokes every unused invite for `username` at `now` and returns how many
/// (group-membership: Removing a user). Used and already revoked invites are
/// left as they are.
pub fn revoke_unused_for(
    tx: &Transaction<'_>,
    username: &str,
    now: SystemTime,
) -> Result<usize, StoreError> {
    Ok(tx.execute(
        "UPDATE invites SET revoked_at = ?2
         WHERE username = ?1 AND reusable = 0 AND used_at IS NULL AND revoked_at IS NULL",
        params![username, unix_millis(now)],
    )?)
}

/// device-identity: Reusable review admin invite. Returns the server's one
/// reusable invite for `username`, creating it at `now` if there is none. It
/// never expires (its `expires_at` is the far future and is never checked) and
/// is never marked used. Only a server with `[review] reusable_admin_invite =
/// true` calls this.
pub fn ensure_reusable(
    tx: &Transaction<'_>,
    username: &str,
    now: SystemTime,
) -> Result<Invite, StoreError> {
    if let Some(invite) = reusable(tx)? {
        if invite.username == username {
            return Ok(invite);
        }
        // `admin_username` changed in the config: the old row names someone
        // who is no longer the admin, so it must stop working.
        tx.execute(
            "UPDATE invites SET revoked_at = ?2 WHERE code = ?1",
            params![invite.code, unix_millis(now)],
        )?;
    }
    let code = new_invite_code()?;
    tx.execute(
        "INSERT INTO invites (code, username, created_at, expires_at, bootstrap, reusable)
         VALUES (?1, ?2, ?3, ?4, 0, 1)",
        params![code, username, unix_millis(now), i64::MAX],
    )?;
    Ok(Invite {
        code,
        username: username.to_owned(),
        created_at: now,
        expires_at: from_unix_millis(i64::MAX),
        bootstrap: false,
    })
}

/// The server's live reusable invite, if it has one (device-identity: Reusable
/// review admin invite).
pub fn reusable(tx: &Transaction<'_>) -> Result<Option<Invite>, StoreError> {
    let row = tx
        .query_row(
            "SELECT code, username, created_at, expires_at FROM invites
             WHERE reusable = 1 AND revoked_at IS NULL
             ORDER BY created_at DESC LIMIT 1",
            [],
            |row| {
                Ok(Invite {
                    code: row.get(0)?,
                    username: row.get(1)?,
                    created_at: from_unix_millis(row.get(2)?),
                    expires_at: from_unix_millis(row.get(3)?),
                    bootstrap: false,
                })
            },
        )
        .optional()?;
    Ok(row)
}

/// Whether `code` is a live reusable invite, and for whom (device-identity:
/// Reusable review admin invite).
pub fn reusable_username(tx: &Transaction<'_>, code: &str) -> Result<Option<String>, StoreError> {
    Ok(tx
        .query_row(
            "SELECT username FROM invites WHERE code = ?1 AND reusable = 1 AND revoked_at IS NULL",
            params![code],
            |row| row.get(0),
        )
        .optional()?)
}
