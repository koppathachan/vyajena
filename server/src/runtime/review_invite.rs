//! server-deployment: Review admin invite hand-off. `server review-invite
//! --config <file.toml>` prints the store reviewers' server's reusable admin
//! invite (device-identity: Reusable review admin invite). Like `admin-reset`,
//! its authority is host access: it opens the configured database and key
//! directory directly. There is no RPC that returns the invite.

use std::time::SystemTime;

use crate::store::{Store, StoreError};

use super::Config;
use super::bootstrap::{self, BootstrapInvite};
use super::server_keys::{KeyError, ServerKeys};

/// Why `review-invite` refused. In every case nothing is printed.
#[derive(Debug, thiserror::Error)]
pub enum ReviewInviteError {
    /// The config does not set `[review] reusable_admin_invite = true`.
    #[error(
        "the review flag is off: this is not the store reviewers' server \
         (`[review] reusable_admin_invite = true` is not set)"
    )]
    FlagOff,
    /// The server has never started here: no key directory yet.
    #[error("no server keys in {0} (start the review server once first)")]
    NoKeys(String),
    /// The server keys could not be read.
    #[error("cannot read the server keys: {0}")]
    Keys(#[from] KeyError),
    /// The configured database could not be opened.
    #[error("cannot open the server database (no access to the data directory?): {0}")]
    OpenDatabase(#[source] StoreError),
    /// A later database error.
    #[error("database error: {0}")]
    Store(#[from] StoreError),
}

/// The reusable admin invite and its payload, read from the server's own
/// database and keys. Creates the invite row if the server has none yet (the
/// same row the server would create at its next start).
pub async fn read(config: &Config, now: SystemTime) -> Result<BootstrapInvite, ReviewInviteError> {
    if !config.review.reusable_admin_invite {
        return Err(ReviewInviteError::FlagOff);
    }
    if !config.key_dir.is_dir() {
        return Err(ReviewInviteError::NoKeys(
            config.key_dir.display().to_string(),
        ));
    }
    let store =
        Store::open_existing(&config.database_path).map_err(ReviewInviteError::OpenDatabase)?;
    let keys = ServerKeys::load_or_create(&config.key_dir)?;
    let invite =
        bootstrap::ensure_reusable_admin_invite(&store, config, keys.tls_spki_sha256(), now).await;
    let _ = store.close().await;
    Ok(invite?)
}
