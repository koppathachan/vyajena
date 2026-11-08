//! Session tokens and the layer every authenticated RPC goes through
//! (wire-transport: Session authentication; pi-server-core task 4.2).
//!
//! `Devices.Authenticate` issues a token with [`issue_session`]. Only its
//! SHA-256 is stored. [`AuthLayer`] sits in front of every service: on every
//! RPC except the four in [`UNAUTHENTICATED_PATHS`] it reads
//! `authorization: Bearer <token>`, looks the token up and rejects a missing,
//! unknown or expired token, or one whose device is revoked, with
//! `UNAUTHENTICATED`. On success it puts the [`Caller`] into the request's
//! extensions, where handlers read it with [`caller`].

use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::SystemTime;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use protocol::device_identity::DeviceId;
use protocol::wire_transport::{
    AUTHORIZATION_BEARER_PREFIX, AUTHORIZATION_METADATA_KEY, SESSION_TOKEN_MAX_LIFETIME,
};
use rusqlite::{OptionalExtension, Transaction, params};
use sha2::{Digest, Sha256};
use tonic::Status;
use tonic::body::Body;
use tonic::codegen::http::{Request, Response};
use tonic::codegen::{BoxFuture, Service};

use crate::runtime::clock::unix_millis;
use crate::runtime::{Clock, Config};
use crate::store::{Store, StoreError};

/// The RPCs that need no session token (wire-transport: Session
/// authentication), as gRPC request paths.
pub const UNAUTHENTICATED_PATHS: &[&str] = &[
    "/vyajena.v1.Devices/GetServerInfo",
    "/vyajena.v1.Devices/Register",
    "/vyajena.v1.Devices/GetChallenge",
    "/vyajena.v1.Devices/Authenticate",
];

/// Random bytes in a session token (256 bits).
pub const SESSION_TOKEN_BYTES: usize = 32;

/// Lifetime of the tokens this Pi issues: the maximum the spec allows.
pub const SESSION_TOKEN_LIFETIME: std::time::Duration = SESSION_TOKEN_MAX_LIFETIME;

/// The authenticated device behind a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    /// The device id (16 bytes).
    pub device_id: DeviceId,
    /// The device's user's username.
    pub username: String,
    /// The user id in its textual form, `<username>@<server_id>`.
    pub user_id: String,
    /// Whether the user is the admin named in the config.
    pub is_admin: bool,
}

/// The [`Caller`] that [`AuthLayer`] attached to `request`. Every handler of an
/// authenticated RPC starts with this; a request without one is refused, so a
/// handler can never run unauthenticated by mistake.
pub fn caller<T>(request: &tonic::Request<T>) -> Result<&Caller, Status> {
    request
        .extensions()
        .get::<Caller>()
        .ok_or_else(|| Status::unauthenticated("missing session token"))
}

/// A new session token: 32 random bytes in base64url without padding.
pub fn new_session_token() -> Result<String, StoreError> {
    let mut bytes = [0u8; SESSION_TOKEN_BYTES];
    getrandom::fill(&mut bytes)
        .map_err(|e| StoreError::Inconsistent(format!("no randomness for session token: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// What is stored for a token: its SHA-256.
pub fn token_hash(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

/// Stores a session for `device_id` issued at `now` and returns the token and
/// its expiry.
pub fn issue_session(
    tx: &Transaction<'_>,
    device_id: &DeviceId,
    now: SystemTime,
) -> Result<(String, SystemTime), StoreError> {
    let token = new_session_token()?;
    let expires_at = now + SESSION_TOKEN_LIFETIME;
    // Expired sessions are no use to anyone; drop them while we are here.
    tx.execute(
        "DELETE FROM sessions WHERE expires_at <= ?1",
        params![unix_millis(now)],
    )?;
    tx.execute(
        "INSERT INTO sessions (token_sha256, device_id, created_at, expires_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            token_hash(&token).as_slice(),
            device_id.as_slice(),
            unix_millis(now),
            unix_millis(expires_at),
        ],
    )?;
    Ok((token, expires_at))
}

/// Deletes every session of `device_id` (revocation).
pub fn end_sessions(tx: &Transaction<'_>, device_id: &DeviceId) -> Result<(), StoreError> {
    tx.execute(
        "DELETE FROM sessions WHERE device_id = ?1",
        params![device_id.as_slice()],
    )?;
    Ok(())
}

/// Revokes `device_id`: it can no longer authenticate, its sessions end and
/// its KeyPackages are deleted. Returns whether it was newly revoked.
pub fn revoke_device(
    tx: &Transaction<'_>,
    device_id: &DeviceId,
    now: SystemTime,
) -> Result<bool, StoreError> {
    let changed = tx.execute(
        "UPDATE devices SET revoked_at = ?2 WHERE device_id = ?1 AND revoked_at IS NULL",
        params![device_id.as_slice(), unix_millis(now)],
    )?;
    end_sessions(tx, device_id)?;
    tx.execute(
        "DELETE FROM key_packages WHERE device_id = ?1",
        params![device_id.as_slice()],
    )?;
    Ok(changed > 0)
}

/// Looks up the unexpired session for `token` at `now` whose device is not
/// revoked.
pub async fn lookup(
    store: &Store,
    config: &Config,
    token: &str,
    now: SystemTime,
) -> Result<Option<Caller>, StoreError> {
    let hash = token_hash(token);
    let found: Option<(Vec<u8>, String)> = store
        .call(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT s.device_id, d.username FROM sessions s
                     JOIN devices d ON d.device_id = s.device_id
                     WHERE s.token_sha256 = ?1 AND s.expires_at > ?2
                       AND d.revoked_at IS NULL",
                    params![hash.as_slice(), unix_millis(now)],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?)
        })
        .await?;
    let Some((device_id, username)) = found else {
        return Ok(None);
    };
    let device_id: DeviceId = device_id
        .as_slice()
        .try_into()
        .map_err(|_| StoreError::Inconsistent("stored device id is not 16 bytes".into()))?;
    Ok(Some(Caller {
        device_id,
        user_id: format!("{username}@{}", config.server_id),
        is_admin: username == config.admin_username,
        username,
    }))
}

/// The bearer token in `authorization` metadata, if well formed.
fn bearer_token<B>(request: &Request<B>) -> Option<&str> {
    request
        .headers()
        .get(AUTHORIZATION_METADATA_KEY)?
        .to_str()
        .ok()?
        .strip_prefix(AUTHORIZATION_BEARER_PREFIX)
        .filter(|token| !token.is_empty())
}

/// Everything the auth layer needs.
#[derive(Clone)]
struct AuthState {
    store: Store,
    config: Arc<Config>,
    clock: Arc<dyn Clock>,
}

/// A tower layer that authenticates every RPC except
/// [`UNAUTHENTICATED_PATHS`] (wire-transport: Session authentication).
#[derive(Clone)]
pub struct AuthLayer {
    state: AuthState,
}

impl AuthLayer {
    /// A layer checking tokens in `store` against `clock`.
    pub fn new(store: Store, config: Arc<Config>, clock: Arc<dyn Clock>) -> Self {
        Self {
            state: AuthState {
                store,
                config,
                clock,
            },
        }
    }
}

impl<S> tower::Layer<S> for AuthLayer {
    type Service = AuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthService {
            inner,
            state: self.state.clone(),
        }
    }
}

/// The service [`AuthLayer`] wraps around the routes.
#[derive(Clone)]
pub struct AuthService<S> {
    inner: S,
    state: AuthState,
}

impl<S> Service<Request<Body>> for AuthService<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = BoxFuture<Self::Response, Self::Error>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        // Take the service that was polled ready and leave a fresh clone.
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let state = self.state.clone();
        Box::pin(async move {
            if UNAUTHENTICATED_PATHS.contains(&request.uri().path()) {
                return inner.call(request).await;
            }
            let Some(token) = bearer_token(&request).map(str::to_owned) else {
                return Ok(Status::unauthenticated("missing session token").into_http());
            };
            let now = state.clock.now();
            match lookup(&state.store, &state.config, &token, now).await {
                Ok(Some(caller)) => {
                    request.extensions_mut().insert(caller);
                    inner.call(request).await
                }
                Ok(None) => {
                    Ok(Status::unauthenticated("invalid or expired session token").into_http())
                }
                Err(error) => {
                    tracing::error!(%error, "session lookup failed");
                    Ok(Status::internal("internal error").into_http())
                }
            }
        })
    }
}
