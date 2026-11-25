//! admin-recovery: Operator break-glass reset. A local `server admin-reset`
//! subcommand returns a locked-out admin to a re-bootstrappable state. Its
//! authority is host access to the server's data directory: it opens the
//! configured database directly (mode 0600, owned by the service user) and
//! refuses if it cannot. In one transaction it marks the configured admin user
//! removed (`users.removed_at`), revokes the admin's existing devices and the
//! admin's unused invites, and changes nothing else — no other user, group, DM,
//! the TLS or server keys, or any stored envelope. It mints no invite and
//! starts no listener. There is deliberately no RPC for this: the only way in
//! is already having the box.

use std::time::SystemTime;

use rusqlite::params;

use crate::admin::invites;
use crate::auth;
use crate::runtime::clock::unix_millis;
use crate::store::{Store, StoreError, records};

use super::Config;

/// What the reset changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetReport {
    /// The admin username that was marked removed.
    pub admin: String,
    /// How many of the admin's devices were newly revoked.
    pub devices_revoked: usize,
    /// How many of the admin's unused invites were revoked.
    pub invites_revoked: usize,
}

/// Why `admin-reset` refused. On any of these the database is left unchanged.
#[derive(Debug, thiserror::Error)]
pub enum ResetError {
    /// A `[recovery] passphrase` is configured and the one given was wrong or
    /// absent (admin-recovery: "Wrong passphrase").
    #[error(
        "the operator passphrase is required and must match (a `[recovery] passphrase` is set in the config)"
    )]
    Passphrase,
    /// The configured admin user does not exist, so there is nothing to reset.
    #[error("no user {0:?} to reset (check `admin_username` in the config)")]
    NoAdmin(String),
    /// The configured database could not be opened: wrong path, or no
    /// permission on the 0600 file (admin-recovery: "No host access, no reset").
    #[error("cannot open the server database (no access to the data directory?): {0}")]
    OpenDatabase(#[source] StoreError),
    /// A later database error while resetting.
    #[error("database error: {0}")]
    Store(#[from] StoreError),
}

/// Checks the operator passphrase, opens the configured database directly and
/// resets the admin. A missing or unreadable database is refused
/// ([`ResetError::OpenDatabase`]), as is a wrong or absent passphrase when one
/// is configured ([`ResetError::Passphrase`]); in both cases nothing changes.
pub async fn run(
    config: &Config,
    passphrase: Option<&str>,
    now: SystemTime,
) -> Result<ResetReport, ResetError> {
    check_passphrase(config, passphrase)?;
    let store = Store::open_existing(&config.database_path).map_err(ResetError::OpenDatabase)?;
    let outcome = reset_admin(&store, config, now).await;
    // Checkpoint the WAL into the database file before returning, regardless.
    let _ = store.close().await;
    outcome
}

/// The passphrase gate. With no `[recovery] passphrase` configured, host access
/// alone is the authority and `passphrase` is ignored. With one configured,
/// `passphrase` must be present and equal to it.
fn check_passphrase(config: &Config, passphrase: Option<&str>) -> Result<(), ResetError> {
    match config.recovery.passphrase.as_deref() {
        None => Ok(()),
        Some(expected) => match passphrase {
            Some(given) if given == expected => Ok(()),
            _ => Err(ResetError::Passphrase),
        },
    }
}

/// The reset transaction against an already-open store: mark the admin removed,
/// revoke the admin's devices and unused invites, and change nothing else.
/// Errors with [`ResetError::NoAdmin`] if the admin user does not exist.
pub async fn reset_admin(
    store: &Store,
    config: &Config,
    now: SystemTime,
) -> Result<ResetReport, ResetError> {
    let admin = config.admin_username.clone();
    let report = store
        .call(move |conn| {
            let tx = conn.transaction()?;
            if records::user(&tx, &admin)?.is_none() {
                return Ok(None);
            }
            tx.execute(
                "UPDATE users SET removed_at = ?2 WHERE username = ?1",
                params![admin, unix_millis(now)],
            )?;
            let mut devices_revoked = 0;
            for device in records::active_devices(&tx, &admin)? {
                if auth::revoke_device(&tx, &device.device_id, now)? {
                    devices_revoked += 1;
                }
            }
            let invites_revoked = invites::revoke_unused_for(&tx, &admin, now)?;
            tx.commit()?;
            Ok(Some(ResetReport {
                admin,
                devices_revoked,
                invites_revoked,
            }))
        })
        .await?;
    report.ok_or_else(|| ResetError::NoAdmin(config.admin_username.clone()))
}
