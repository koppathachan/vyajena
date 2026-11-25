//! One gRPC channel over one pinned TLS connection, and the session token
//! that authenticates calls on it (wire-transport: Session authentication).

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use hyper_util::rt::TokioIo;
use protocol::wire_transport::{
    AUTH_TLS_EXPORTER_LEN, AUTHORIZATION_BEARER_PREFIX, AUTHORIZATION_METADATA_KEY,
    SESSION_TOKEN_MAX_LIFETIME, sign_auth_challenge,
};
use tonic::transport::{Channel, Endpoint};

use super::NetError;
use super::tls;
use crate::identity::DeviceKeys;
use crate::identity::certificate::from_timestamp;
use crate::pb;
use crate::pb::admin_client::AdminClient;
use crate::pb::blobs_client::BlobsClient;
use crate::pb::devices_client::DevicesClient;
use crate::pb::dms_client::DmsClient;
use crate::pb::keys_client::KeysClient;
use crate::pb::messaging_client::MessagingClient;
use crate::pb::prefs_client::PrefsClient;

/// A token is renewed once it has less than this left (client-runtime:
/// Connection lifecycle: renew the session token before it expires).
pub const TOKEN_RENEW_BEFORE: Duration = Duration::from_secs(5 * 60);

/// One HTTP/2 connection to the server over pinned TLS. It does not reconnect:
/// when it breaks, the engine opens a new one.
#[derive(Clone)]
pub struct Conn {
    channel: Channel,
    exporter: [u8; AUTH_TLS_EXPORTER_LEN],
}

impl std::fmt::Debug for Conn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Conn").finish_non_exhaustive()
    }
}

impl Conn {
    /// Connects to `address` over TLS pinned to `pin` and starts HTTP/2 on it.
    pub async fn open(address: &str, pin: [u8; 32]) -> Result<Self, NetError> {
        let tls::PinnedStream { stream, exporter } = tls::connect(address, pin).await?;
        let slot = Arc::new(Mutex::new(Some(stream)));
        let channel = Endpoint::from_static("http://vyajena2-server")
            .connect_with_connector(tower::service_fn(move |_| {
                let stream = slot.lock().unwrap_or_else(PoisonError::into_inner).take();
                async move {
                    stream
                        .map(TokioIo::new)
                        .ok_or_else(|| std::io::Error::other("the connection is already in use"))
                }
            }))
            .await
            .map_err(|e| NetError::Connect(format!("HTTP/2: {e}")))?;
        Ok(Self { channel, exporter })
    }

    /// This connection's TLS exporter value.
    pub fn exporter(&self) -> &[u8; AUTH_TLS_EXPORTER_LEN] {
        &self.exporter
    }

    /// A `Devices` client.
    pub fn devices(&self) -> DevicesClient<Channel> {
        DevicesClient::new(self.channel.clone())
    }

    /// A `Keys` client.
    pub fn keys(&self) -> KeysClient<Channel> {
        KeysClient::new(self.channel.clone())
    }

    /// An `Admin` client.
    pub fn admin(&self) -> AdminClient<Channel> {
        AdminClient::new(self.channel.clone())
    }

    /// A `Blobs` client (change `media-blobs`).
    pub fn blobs(&self) -> BlobsClient<Channel> {
        BlobsClient::new(self.channel.clone())
    }

    /// A `Dms` client (change `dms`).
    pub fn dms(&self) -> DmsClient<Channel> {
        DmsClient::new(self.channel.clone())
    }

    /// A `Prefs` client (notification-prefs, change `mute-notifications`).
    pub fn prefs(&self) -> PrefsClient<Channel> {
        PrefsClient::new(self.channel.clone())
    }

    /// A `Messaging` client.
    pub fn messaging(&self) -> MessagingClient<Channel> {
        MessagingClient::new(self.channel.clone())
    }

    /// `Devices.GetServerInfo`.
    pub async fn server_info(&self) -> Result<pb::ServerInfo, NetError> {
        self.devices()
            .get_server_info(pb::GetServerInfoRequest {})
            .await?
            .into_inner()
            .server_info
            .ok_or_else(|| NetError::Protocol("GetServerInfo without server_info".into()))
    }

    /// `GetChallenge` then `Authenticate` with the device auth key, signing
    /// the nonce and this connection's TLS exporter value (wire-transport:
    /// Session authentication). Returns a session.
    /// `now` is the client's clock when the call starts; renewal is timed
    /// from it, so a device clock that differs from the server's does not make
    /// the core renew on every tick.
    pub async fn authenticate(
        &self,
        keys: &DeviceKeys,
        device: pb::DeviceRef,
        now: SystemTime,
    ) -> Result<Session, NetError> {
        let nonce = self
            .devices()
            .get_challenge(pb::GetChallengeRequest {})
            .await?
            .into_inner()
            .nonce;
        let signature = sign_auth_challenge(keys.auth(), &nonce, &self.exporter).to_vec();
        let response = self
            .devices()
            .authenticate(pb::AuthenticateRequest {
                device: Some(device),
                nonce,
                signature,
            })
            .await?
            .into_inner();
        let expires_at = response
            .expires_at
            .as_ref()
            .map(from_timestamp)
            .ok_or_else(|| NetError::Protocol("Authenticate without expires_at".into()))?;
        tracing::debug!("authenticated");
        // The lifetime as the server states it, at most the protocol maximum; a
        // device clock ahead of the server's falls back to the maximum.
        let lifetime = expires_at
            .duration_since(now)
            .unwrap_or(SESSION_TOKEN_MAX_LIFETIME)
            .min(SESSION_TOKEN_MAX_LIFETIME);
        Ok(Session {
            conn: self.clone(),
            token: response.session_token,
            expires_at,
            renew_at: now + lifetime.saturating_sub(TOKEN_RENEW_BEFORE),
        })
    }

    /// Binds a token kept from an earlier connection to this one, so the stream
    /// can open with it and no `GetChallenge`/`Authenticate` is sent
    /// (wire-transport: Session resume). The token's expiry and renewal point
    /// are carried over unchanged; the server accepts it on this connection because
    /// a token is not tied to the connection that minted it (design D1).
    pub fn resume(&self, resume: &ResumeToken) -> Session {
        Session {
            conn: self.clone(),
            token: resume.token.clone(),
            expires_at: resume.expires_at,
            renew_at: resume.renew_at,
        }
    }
}

/// An authenticated connection: the channel and a session token.
#[derive(Clone)]
pub struct Session {
    conn: Conn,
    token: String,
    expires_at: SystemTime,
    renew_at: SystemTime,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

impl Session {
    /// The connection.
    pub fn conn(&self) -> &Conn {
        &self.conn
    }

    /// When the token expires (the server's time, compared with the client's
    /// clock).
    pub fn expires_at(&self) -> SystemTime {
        self.expires_at
    }

    /// Whether the token should be renewed at `now` (client clock):
    /// [`TOKEN_RENEW_BEFORE`] before it expires.
    pub fn needs_renewal(&self, now: SystemTime) -> bool {
        now >= self.renew_at
    }

    /// `message` as a request with `authorization: Bearer <token>`.
    pub fn request<T>(&self, message: T) -> Result<tonic::Request<T>, NetError> {
        let mut request = tonic::Request::new(message);
        let value = format!("{AUTHORIZATION_BEARER_PREFIX}{}", self.token)
            .parse()
            .map_err(|_| NetError::Protocol("the session token is not valid metadata".into()))?;
        request
            .metadata_mut()
            .insert(AUTHORIZATION_METADATA_KEY, value);
        Ok(request)
    }

    /// The token and its timing, to keep across a reconnect and resume with
    /// (wire-transport: Session resume; client-runtime: Connection lifecycle).
    pub fn to_resume(&self) -> ResumeToken {
        ResumeToken {
            token: self.token.clone(),
            expires_at: self.expires_at,
            renew_at: self.renew_at,
        }
    }
}

/// A session token kept across a reconnect so the core can open the next
/// `Connect` stream without `GetChallenge`/`Authenticate` (wire-transport:
/// Session resume). It is independent of the connection that minted it: the server
/// authenticates `Connect` from the token alone (design D1).
#[derive(Clone)]
pub struct ResumeToken {
    token: String,
    expires_at: SystemTime,
    renew_at: SystemTime,
}

impl std::fmt::Debug for ResumeToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResumeToken")
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

/// First byte of a saved [`ResumeToken`]; a value with another first byte is
/// not read (client-runtime: Resume across restart).
const RESUME_VERSION: u8 = 1;

impl ResumeToken {
    /// The saved form, for the key store (client-runtime: Resume across
    /// restart): the version byte, `expires_at` and `renew_at` as seconds
    /// since the Unix epoch (big-endian `u64`), then the token's bytes.
    pub fn encode(&self) -> zeroize::Zeroizing<Vec<u8>> {
        let mut out = zeroize::Zeroizing::new(Vec::with_capacity(17 + self.token.len()));
        out.push(RESUME_VERSION);
        out.extend_from_slice(&unix_secs(self.expires_at).to_be_bytes());
        out.extend_from_slice(&unix_secs(self.renew_at).to_be_bytes());
        out.extend_from_slice(self.token.as_bytes());
        out
    }

    /// A token from its saved form; `None` for anything this version did not
    /// write (unknown version, too short, not UTF-8, empty token), which the
    /// core treats as no token.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let (&version, rest) = bytes.split_first()?;
        if version != RESUME_VERSION || rest.len() <= 16 {
            return None;
        }
        let (expires, rest) = rest.split_at(8);
        let (renew, token) = rest.split_at(8);
        let expires = u64::from_be_bytes(expires.try_into().ok()?);
        let renew = u64::from_be_bytes(renew.try_into().ok()?);
        Some(Self {
            token: std::str::from_utf8(token).ok()?.to_owned(),
            expires_at: SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(expires))?,
            renew_at: SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(renew))?,
        })
    }

    /// Whether `other` is the same session token (not the same times).
    pub fn same_token(&self, other: &Self) -> bool {
        self.token == other.token
    }

    /// Whether this token is worth resuming with at `now`: still before its
    /// renewal point, so a resumed session does not land already due for
    /// renewal (design D2).
    pub fn usable(&self, now: SystemTime) -> bool {
        now < self.renew_at
    }
}

fn unix_secs(time: SystemTime) -> u64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}
