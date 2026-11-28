//! The public [`Client`] (client-core design D2).
//!
//! A device's life: [`Client::open`]; then once either [`Client::register`]
//! (first device, from an invite) or [`Client::link_request`] +
//! [`Client::complete_link`] (another device of a user); then
//! [`Client::start`], which connects and keeps the device in the group. Admin
//! actions ([`Client::create_group`], [`Client::add_user`],
//! [`Client::remove_user`], invites) and linking other devices need the
//! client to be started and connected. Messaging ([`Client::send_text`],
//! [`Client::set_profile`], [`Client::rename_group`]) stores the
//! message first and sends it whenever the engine can, so it works offline;
//! its status arrives as [`Event::SendStatus`].

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use parking_lot::Mutex;
use protocol::wire_transport::{SUPPORTED_PROTOCOL_VERSIONS, negotiate_protocol_version};
use tokio::sync::{mpsc, oneshot};

use crate::clock::{Clock, SystemClock};
use crate::engine::{Command, Engine, Job, Shared};
use crate::events::{
    self, Chat, ChatId, ChatKind, DeviceId, Event, EventStream, InviteInfo, Member, Message,
    MessageId, MutePrefs, MuteTarget, Presence, Profile, PushPlatform, PushToken,
    ReplyOriginalKind, ReportTarget, UserId,
};
use crate::identity::certificate::{self, from_timestamp};
use crate::identity::keys::{create_identity_key, identity_key};
use crate::identity::{DeviceKeys, Invite, LinkApproval, LinkRequest, PayloadError};
use crate::keystore::{KeyName, KeyStore, KeyStoreError, KeyStoreKind};
use crate::media::{MediaCache, MediaError};
use crate::messaging::ContentError;
use crate::mls::MlsError;
use crate::mls::verify;
use crate::moderation::{Report, ReportKind};
use crate::net::{Conn, NetError, Session};
use crate::notification::{ConversationKind, NotificationSummary};
use crate::pb;
use crate::store::{AccountRow, OutgoingMessage, ProfileRow, Store, StoreError};

/// The database file inside [`ClientConfig::data_dir`].
pub const DATABASE_FILE: &str = "vyajena2.sqlite";

/// How to open a [`Client`].
#[derive(Clone)]
pub struct ClientConfig {
    /// Where the database lives.
    pub data_dir: PathBuf,
    /// Where the keys live: from [`crate::keystore::open_key_store`], or a
    /// [`crate::keystore::MemoryKeyStore`] in tests.
    pub key_store: Arc<dyn KeyStore>,
    /// The clock; [`SystemClock`] unless a test moves time itself (design D7).
    pub clock: Arc<dyn Clock>,
    /// Where decrypted images are cached, or `None` for `media` inside
    /// [`ClientConfig::data_dir`]. The app points it at a folder the OS
    /// encrypts at rest and leaves out of backups (client-runtime: Image
    /// download and cache).
    pub media_dir: Option<PathBuf>,
    /// The most bytes the media cache holds before it drops the least recently
    /// shown images.
    pub media_cache_bytes: u64,
}

impl ClientConfig {
    /// A config on the system clock.
    pub fn new(data_dir: impl Into<PathBuf>, key_store: Arc<dyn KeyStore>) -> Self {
        Self {
            data_dir: data_dir.into(),
            key_store,
            clock: Arc::new(SystemClock),
            media_dir: None,
            media_cache_bytes: crate::media::DEFAULT_CACHE_BYTES,
        }
    }

    /// The same config with the media cache in `dir`.
    pub fn with_media_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.media_dir = Some(dir.into());
        self
    }

    /// The same config with the media cache capped at `bytes`.
    pub fn with_media_cache_bytes(mut self, bytes: u64) -> Self {
        self.media_cache_bytes = bytes;
        self
    }

    /// The same config on `clock`.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }
}

impl std::fmt::Debug for ClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientConfig")
            .field("data_dir", &self.data_dir)
            .field("key_store", &self.key_store.kind())
            .field("media_dir", &self.media_dir)
            .finish_non_exhaustive()
    }
}

/// Why a [`Client`] call failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ClientError {
    /// The key store failed, or there is none (see
    /// [`KeyStoreError::NoKeychain`]).
    #[error(transparent)]
    KeyStore(#[from] KeyStoreError),
    /// The local store failed.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A blocking task could not run.
    #[error("internal task failed: {0}")]
    Task(String),
    /// Talking to the server failed; [`NetError::ServerIdentity`] means the pin
    /// did not match.
    #[error(transparent)]
    Net(#[from] NetError),
    /// OpenMLS failed.
    #[error(transparent)]
    Mls(#[from] MlsError),
    /// An invite, link request or link approval is not valid.
    #[error(transparent)]
    Payload(#[from] PayloadError),
    /// This device has no account yet (register or link first).
    #[error("this device is not registered")]
    NotRegistered,
    /// This device already has an account.
    #[error("this device is already registered")]
    AlreadyRegistered,
    /// The client is not started or not connected right now.
    #[error("not connected to the server")]
    Offline,
    /// Only the admin's devices may do this.
    #[error("only the admin can do this")]
    NotAdmin,
    /// This device holds no user identity key (it is not the user's first
    /// device), so it cannot certify another device.
    #[error("this device does not hold the user identity key")]
    NotIdentityDevice,
    /// `complete_link` without a `link_request` first.
    #[error("no link request was made on this device")]
    NoLinkRequest,
    /// The server behind the invite is not the one it names.
    #[error("the server does not match the invite: {0}")]
    ServerMismatch(String),
    /// The pasted invite has expired; a fresh one is needed (device-identity:
    /// Signup with an invite; app-ui: Invite expiry is surfaced).
    #[error("the invite has expired")]
    InviteExpired,
    /// This device is not in the group.
    #[error("this device is not in the group")]
    NoGroup,
    /// The group already exists.
    #[error("the group already exists")]
    AlreadyInGroup,
    /// This device refused a commit and cannot follow the group until it is
    /// re-added.
    #[error("this device stopped following the group: {0}")]
    Stalled(String),
    /// A user's identity key differs from the pinned one; nothing of theirs
    /// is added until the user accepts the new key
    /// ([`Client::accept_identity_key`]).
    #[error("the identity key of {0} changed")]
    IdentityKeyChanged(UserId),
    /// A fetched certificate or KeyPackage did not check out.
    #[error("a certificate does not check out: {0}")]
    Certificate(String),
    /// The server knows no such user.
    #[error("unknown user {0}")]
    UnknownUser(UserId),
    /// The user has no KeyPackages on the server yet, so they cannot be added.
    #[error("{0} has no KeyPackages yet")]
    NoKeyPackages(UserId),
    /// The commit would be larger than the handshake limit.
    #[error("the commit is larger than the server accepts")]
    TooLarge,
    /// The message would encrypt to more than the 65,536-byte ciphertext cap
    /// (message-content: Text only).
    #[error("the message is too long")]
    TooLong,
    /// A mention's range does not fit the text (message-content: Mentions).
    #[error("a mention does not fit the text")]
    InvalidMention,
    /// An image could not be prepared, sent or opened (message-content: Image
    /// messages).
    #[error(transparent)]
    Media(#[from] MediaError),
    /// The server no longer has the picture: its blob expired or was deleted
    /// (client-runtime: Expired media).
    #[error("the picture has expired")]
    MediaExpired,
    /// This device no longer has the picture to resend.
    #[error("the picture is not on this device any more")]
    MediaNotOnDevice,
    /// A video is longer than the 30-second cap (video-messages: Config-driven
    /// limit); the client enforces duration, which the server never sees.
    #[error("the video is longer than the limit")]
    VideoTooLong,
    /// A video's encrypted blob is over the server's advertised maximum blob size
    /// (video-messages: Config-driven limit).
    #[error("the video is larger than the server accepts")]
    VideoTooLarge,
    /// The bytes handed in are not an MP4 clip (video-messages: Video messages).
    #[error("this is not an MP4 video the app can send")]
    NotAVideo,
    /// No stored chat message has that id (client-runtime: Reply lookup).
    #[error("no such message")]
    NoSuchMessage,
    /// The server rejected the commit.
    #[error("the server rejected the commit: {0}")]
    Rejected(String),
    /// Not possible yet; retried later.
    #[error("not possible yet")]
    NotYet,

    /// No chat with that id (change `dms`).
    #[error("no such chat")]
    NoSuchChat,

    /// The server closed this DM: its peer was removed (direct-messages: DM
    /// closed on user removal).
    #[error("this DM is closed")]
    ChatClosed,

    /// The server does not know named groups yet (it lists no protocol
    /// version 3).
    #[error("the server does not support groups yet")]
    GroupsUnavailable,

    /// The server does not know DMs yet (it lists no protocol version 2).
    #[error("the server does not support DMs yet")]
    DmsUnavailable,

    /// This device may not delete that message (message-deletion: Who may
    /// delete): a non-admin in the lobby or a named group, or another user's
    /// message in a DM.
    #[error("this device may not delete that message")]
    CannotDelete,

    /// The admin account cannot be deleted from the app (account-deletion: The
    /// admin cannot self-delete through the RPC): its only reset path is the
    /// operator-only `server admin-reset` on the host.
    #[error("the admin account cannot be deleted from the app")]
    AdminCannotDelete,
}

struct Running {
    commands: mpsc::Sender<Command>,
    task: tokio::task::JoinHandle<()>,
}

/// The device core: one per device and data directory.
pub struct Client {
    shared: Arc<Shared>,
    stream: Option<EventStream>,
    running: Mutex<Option<Running>>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("key_store", &self.shared.key_store.kind())
            .finish_non_exhaustive()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Some(running) = self.running.lock().take() {
            running.task.abort();
        }
    }
}

/// Runs a key store call off the async runtime (it may wait on a prompt).
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, ClientError> + Send + 'static,
) -> Result<T, ClientError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|error| ClientError::Task(error.to_string()))?
}

/// The session token the last run saved, to resume with (client-runtime: Resume
/// across restart). A key store error or a value this version cannot read is
/// no token: the core then runs the full handshake.
fn load_resume(store: &dyn KeyStore) -> Option<crate::net::ResumeToken> {
    match store.get(KeyName::SessionToken) {
        Ok(Some(saved)) => {
            let token = crate::net::ResumeToken::decode(&saved);
            if token.is_none() {
                tracing::warn!("the saved resume token is not readable; ignoring it");
            }
            token
        }
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(%error, "could not read the saved resume token");
            None
        }
    }
}

/// `Devices.GetServerInfo`, checked against the invite's server id and this
/// core's protocol version.
async fn checked_server_info(conn: &Conn, server_id: &str) -> Result<pb::ServerInfo, ClientError> {
    let info = conn.server_info().await?;
    if info.server_id != server_id {
        return Err(ClientError::ServerMismatch(format!(
            "it is {:?}, the invite names {server_id:?}",
            info.server_id
        )));
    }
    if negotiate_protocol_version(&info.supported_protocol_versions).is_none() {
        return Err(ClientError::ServerMismatch(format!(
            "it speaks none of the protocol versions {SUPPORTED_PROTOCOL_VERSIONS:?}"
        )));
    }
    Ok(info)
}

impl Client {
    /// Opens the client: the database in `data_dir` (created if needed) and
    /// the storage key in the key store (created on first start). The key
    /// store may block, e.g. on a macOS keychain prompt, so this runs off the
    /// async runtime.
    pub async fn open(config: ClientConfig) -> Result<Self, ClientError> {
        let ClientConfig {
            data_dir,
            key_store,
            clock,
            media_dir,
            media_cache_bytes,
        } = config;
        let media = MediaCache::new(
            media_dir.unwrap_or_else(|| data_dir.join("media")),
            media_cache_bytes,
        );
        let keys = Arc::clone(&key_store);
        let store =
            blocking(move || Ok(Store::open(&data_dir.join(DATABASE_FILE), keys.as_ref())?))
                .await?;
        let (events, stream) = events::channel();
        tracing::info!(key_store = ?key_store.kind(), "client opened");
        Ok(Self {
            shared: Arc::new(Shared {
                store: Arc::new(store),
                key_store,
                clock,
                events,
                session: Mutex::new(None),
                server_knows_dms: std::sync::atomic::AtomicBool::new(false),
                server_knows_named_groups: std::sync::atomic::AtomicBool::new(false),
                server_knows_presence: std::sync::atomic::AtomicBool::new(false),
                max_video_blob_bytes: std::sync::atomic::AtomicU64::new(0),
                media,
            }),
            stream: Some(stream),
            running: Mutex::new(None),
        })
    }

    /// The one ordered event stream (client-runtime: Events in order). There
    /// is only one: the first call gets it, later calls get `None`.
    pub fn take_events(&mut self) -> Option<EventStream> {
        self.stream.take()
    }

    /// Where this client's keys live.
    pub fn key_store_kind(&self) -> KeyStoreKind {
        self.shared.key_store.kind()
    }

    /// The time on this client's clock.
    pub fn now(&self) -> SystemTime {
        self.shared.clock.now()
    }

    fn account(&self) -> Result<AccountRow, ClientError> {
        self.shared
            .store
            .account()?
            .ok_or(ClientError::NotRegistered)
    }

    /// This device's user, once registered or linked.
    pub fn user_id(&self) -> Result<Option<UserId>, ClientError> {
        Ok(self
            .shared
            .store
            .account()?
            .map(|a| UserId::new(a.username, a.server_id)))
    }

    /// This device's id, once registered or linked.
    pub fn device_id(&self) -> Result<Option<DeviceId>, ClientError> {
        Ok(self.shared.store.account()?.map(|a| a.device_id))
    }

    /// Whether this device's user is the admin (device-identity: Admin
    /// awareness): the admin's app creates the group with
    /// [`Client::create_group`].
    pub fn is_admin(&self) -> Result<bool, ClientError> {
        Ok(self.account()?.is_admin)
    }

    // --- Signup and linking (device-identity) ---

    /// Signs up this device as a user's first device with an invite payload
    /// (device-identity: Signup with an invite): creates the user identity
    /// key, the device auth and MLS keys (in the key store only), a
    /// certificate, calls `Devices.Register`, authenticates, learns whether
    /// the user is the admin, and uploads KeyPackages. Returns the user id
    /// the invite created. Call [`Client::start`] next.
    pub async fn register(&self, invite: &[u8]) -> Result<UserId, ClientError> {
        if self.shared.store.account()?.is_some() {
            return Err(ClientError::AlreadyRegistered);
        }
        let invite = Invite::decode(invite)?;
        let store = Arc::clone(&self.shared.key_store);
        let (keys, identity) = blocking(move || {
            // device-identity: Fresh keys for a new signup.
            let keys = DeviceKeys::create_fresh(store.as_ref())?;
            let identity = create_identity_key(store.as_ref())?;
            Ok((keys, identity))
        })
        .await?;
        let signed = certificate::sign(
            &identity,
            &invite.user,
            &keys.auth_public(),
            &keys.mls_public(),
            self.now(),
        );
        let conn = self
            .open_conn(&invite.relay_address, invite.tls_pin)
            .await?;
        let info = checked_server_info(&conn, &invite.user.server_id).await?;
        let identity_public = identity.verifying_key().to_bytes().to_vec();
        let registered = match conn
            .devices()
            .register(pb::RegisterRequest {
                invite_code: invite.code.clone(),
                identity_public_key: identity_public.clone(),
                certificate: Some(signed.clone()),
            })
            .await
        {
            Ok(response) => response.into_inner(),
            // device-identity: Signup with an invite. The server marks an
            // expired-but-otherwise-valid invite with DEADLINE_EXCEEDED so the
            // app can say it expired (app-ui: Invite expiry is surfaced).
            Err(status) if status.code() == tonic::Code::DeadlineExceeded => {
                return Err(ClientError::InviteExpired);
            }
            Err(status) => return Err(NetError::from(status).into()),
        };
        let user = registered
            .user_id
            .as_ref()
            .map(UserId::from_pb)
            .ok_or_else(|| NetError::Protocol("Register without user_id".into()))?;
        if user != invite.user {
            return Err(ClientError::ServerMismatch(format!(
                "registered as {user}, the invite names {}",
                invite.user
            )));
        }
        tracing::info!(user = %user, "registered");
        self.finish_signup(
            &conn,
            &keys,
            &user,
            &invite.relay_address,
            invite.tls_pin,
            &info,
            &identity_public,
        )
        .await?;
        Ok(user)
    }

    /// The new device's half of linking, step 1 (device-identity: Device
    /// linking): creates the device auth and MLS keys and returns the link
    /// request to show as a QR code or text on this device.
    pub async fn link_request(&self) -> Result<Vec<u8>, ClientError> {
        if self.shared.store.account()?.is_some() {
            return Err(ClientError::AlreadyRegistered);
        }
        let store = Arc::clone(&self.shared.key_store);
        // device-identity: Fresh keys for a new signup. Each request starts
        // over; a leftover identity key would make this device act as the
        // user's identity device.
        let keys = blocking(move || {
            store.delete(KeyName::IdentityKey)?;
            Ok(DeviceKeys::create_fresh(store.as_ref())?)
        })
        .await?;
        Ok(LinkRequest {
            auth_public_key: keys.auth_public(),
            mls_signature_public_key: keys.mls_public(),
        }
        .encode())
    }

    /// On the device that holds the user identity key: certifies the device
    /// that showed `request`, uploads its certificate with
    /// `Devices.LinkDevice`, and returns the link approval to show back to it.
    /// This device adds the new one to the group as soon as it has uploaded
    /// KeyPackages. Needs the client started and connected.
    pub async fn approve_link(&self, request: &[u8]) -> Result<Vec<u8>, ClientError> {
        let account = self.account()?;
        let request = LinkRequest::decode(request)?;
        let store = Arc::clone(&self.shared.key_store);
        let identity =
            blocking(move || identity_key(store.as_ref())?.ok_or(ClientError::NotIdentityDevice))
                .await?;
        let user = UserId::new(account.username.clone(), account.server_id.clone());
        let signed = certificate::sign(
            &identity,
            &user,
            &request.auth_public_key,
            &request.mls_signature_public_key,
            self.now(),
        );
        let session = self.session()?;
        session
            .conn()
            .devices()
            .link_device(session.request(pb::LinkDeviceRequest {
                certificate: Some(signed),
            })?)
            .await
            .map_err(NetError::from)?;
        self.shared
            .store
            .add_pending_link(&request.device_id(), self.now())?;
        tracing::info!("linked a new device");
        Ok(LinkApproval {
            relay_address: account.relay_address,
            tls_pin: account.tls_pin,
            user,
        }
        .encode())
    }

    /// The new device's half of linking, step 2: with the approval the
    /// identity device showed, authenticates with this device's own auth key
    /// (no invite), reads its certificate with `Devices.GetSelf`, pins its
    /// user's identity key and uploads KeyPackages. Call [`Client::start`]
    /// next; the identity device then adds this one to the group.
    pub async fn complete_link(&self, approval: &[u8]) -> Result<UserId, ClientError> {
        if self.shared.store.account()?.is_some() {
            return Err(ClientError::AlreadyRegistered);
        }
        let approval = LinkApproval::decode(approval)?;
        let store = Arc::clone(&self.shared.key_store);
        let keys =
            blocking(move || DeviceKeys::load(store.as_ref())?.ok_or(ClientError::NoLinkRequest))
                .await?;
        let conn = self
            .open_conn(&approval.relay_address, approval.tls_pin)
            .await?;
        let info = checked_server_info(&conn, &approval.user.server_id).await?;
        let session = conn
            .authenticate(
                &keys,
                crate::membership::device_ref(&approval.user, &keys.device_id()),
                self.now(),
            )
            .await?;
        // The user's identity key, from the server (pinned on first sight), with
        // `Keys.GetIdentityKeys` (key-directory: Identity key lookup).
        let identity = crate::keys::identity_keys(&session, std::slice::from_ref(&approval.user))
            .await?
            .remove(&approval.user)
            .ok_or_else(|| {
                NetError::Protocol("the server has no identity key for this user".into())
            })?;
        self.finish_signup(
            &conn,
            &keys,
            &approval.user,
            &approval.relay_address,
            approval.tls_pin,
            &info,
            &identity,
        )
        .await?;
        tracing::info!(user = %approval.user, "linked to an existing user");
        Ok(approval.user)
    }

    async fn open_conn(&self, address: &str, pin: [u8; 32]) -> Result<Conn, ClientError> {
        match Conn::open(address, pin).await {
            Ok(conn) => Ok(conn),
            Err(NetError::ServerIdentity) => {
                self.shared.events.emit(Event::ServerIdentityError);
                Err(NetError::ServerIdentity.into())
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Shared end of signup and linking: authenticate, `GetSelf`, check the
    /// certificate, pin the own identity key, save the account, upload
    /// KeyPackages.
    #[allow(clippy::too_many_arguments)]
    async fn finish_signup(
        &self,
        conn: &Conn,
        keys: &DeviceKeys,
        user: &UserId,
        relay_address: &str,
        tls_pin: [u8; 32],
        info: &pb::ServerInfo,
        identity_public: &[u8],
    ) -> Result<(), ClientError> {
        let session = conn
            .authenticate(
                keys,
                crate::membership::device_ref(user, &keys.device_id()),
                self.now(),
            )
            .await?;
        let me = session
            .conn()
            .devices()
            .get_self(session.request(pb::GetSelfRequest {})?)
            .await
            .map_err(NetError::from)?
            .into_inner();
        let signed = me
            .certificate
            .ok_or_else(|| NetError::Protocol("GetSelf without a certificate".into()))?;
        let cert = certificate::verify(&signed, identity_public)
            .map_err(|problem| ClientError::Certificate(problem.to_string()))?;
        if cert.user != *user
            || cert.device_id != keys.device_id()
            || cert.mls_signature_public_key != keys.mls_public()
        {
            return Err(ClientError::Certificate(
                "the server's certificate for this device is not this device's".into(),
            ));
        }
        let sender = info
            .external_sender
            .as_ref()
            .ok_or_else(|| NetError::Protocol("ServerInfo without the external sender".into()))?;
        let now = self.now();
        let store = &self.shared.store;
        store.pin_identity_key(&user.to_string(), identity_public, now)?;
        store.save_account(&AccountRow {
            relay_address: relay_address.to_owned(),
            tls_pin,
            username: user.username.clone(),
            server_id: user.server_id.clone(),
            device_id: keys.device_id(),
            certificate: signed.certificate.clone(),
            certificate_signature: signed.signature.clone(),
            is_admin: me.is_admin,
            pi_sender_key: sender.signature_public_key.clone(),
            pi_sender_credential: sender.credential.clone(),
            created_at: now,
        })?;
        // key-directory: KeyPackage upload, so the device can be added.
        let credential = certificate::credential_with_key(&signed, &keys.mls_public());
        let upload = store.transaction(|| -> Result<_, ClientError> {
            Ok(crate::keys::new_upload(
                store.mls(),
                &keys.mls_signer(),
                &credential,
                protocol::key_directory::ONE_TIME_KEY_PACKAGES_TARGET,
            )?)
        })?;
        session
            .conn()
            .keys()
            .upload_key_packages(session.request(pb::UploadKeyPackagesRequest {
                key_packages: upload.one_time,
                last_resort_key_package: upload.last_resort,
            })?)
            .await
            .map_err(NetError::from)?;
        store.add_last_resort(&upload.last_resort_ref, now)?;
        // direct-messages: Old apps never join a DM. These list 0xF0A2
        // already, so the first start needs no second upload.
        store.set_flag(crate::engine::DM_KEY_PACKAGES_FLAG, now)?;
        Ok(())
    }

    // --- Running ---

    /// Starts the engine: connects to the relay from the invite with the
    /// pinned TLS key, authenticates, keeps one `Connect` stream open with
    /// heartbeats, renews the session token and reconnects with backoff
    /// (client-runtime: Connection lifecycle). Calling it again restarts it,
    /// e.g. after a [`Event::ServerIdentityError`].
    pub async fn start(&self) -> Result<(), ClientError> {
        let account = self.account()?;
        let store = Arc::clone(&self.shared.key_store);
        let (keys, resume) = blocking(move || {
            let keys = DeviceKeys::load(store.as_ref())?.ok_or(ClientError::NotRegistered)?;
            Ok((keys, load_resume(store.as_ref())))
        })
        .await?;
        self.stop();
        let (commands, receiver) = mpsc::channel(64);
        let engine = Engine::new(
            Arc::clone(&self.shared),
            keys,
            account,
            receiver,
            commands.clone(),
            resume,
        )
        .map_err(ClientError::from)?;
        let task = tokio::spawn(engine.run());
        *self.running.lock() = Some(Running { commands, task });
        Ok(())
    }

    /// Stops the engine and closes the connection.
    ///
    /// The engine stops at once, wherever it is: everything it does is
    /// stored transactionally, and an own commit in flight is resent by the
    /// next [`Client::start`].
    pub fn stop(&self) {
        if let Some(running) = self.running.lock().take() {
            running.task.abort();
            *self.shared.session.lock() = None;
            self.shared.events.emit(Event::Connection(
                crate::events::ConnectionState::Disconnected,
            ));
        }
    }

    /// The network is back (the app saw a connectivity change): if the engine
    /// is waiting out a backoff delay, it connects at once. Does nothing while
    /// connected, before [`Client::start`] or after [`Event::Removed`] /
    /// [`Event::ServerIdentityError`] (client-runtime: Connection lifecycle).
    pub fn reconnect_now(&self) {
        if let Some(running) = self.running.lock().as_ref() {
            // A full queue means the engine is busy and not waiting.
            let _ = running.commands.try_send(Command::ReconnectNow);
        }
    }

    /// Whether the client has an authenticated connection right now.
    pub fn is_connected(&self) -> bool {
        self.shared.session.lock().is_some()
    }

    /// Re-sync a chat that the core signalled is out of sync: this device is
    /// wedged behind the group's epoch and cannot self-recover. Asks the server
    /// to be re-added (bounded by the per-device daily limit), even though this
    /// device still holds group state (group-membership: Orphaned device
    /// self-recovery; app-ui: Out-of-sync re-sync affordance). The held message
    /// keeps its `msg_id` and is sent once a device able to re-add this one is
    /// online. Does nothing before [`Client::start`].
    pub fn request_resync(&self) {
        if let Some(running) = self.running.lock().as_ref() {
            let _ = running.commands.try_send(Command::Job(Job::RequestReadd {
                refused_seq: 0,
                reason: "re-sync".into(),
                wedged: true,
            }));
        }
    }

    fn session(&self) -> Result<Session, ClientError> {
        self.shared
            .session
            .lock()
            .clone()
            .ok_or(ClientError::Offline)
    }

    async fn job(
        &self,
        make: impl FnOnce(Option<crate::engine::Reply>) -> Job,
    ) -> Result<(), ClientError> {
        let commands = self
            .running
            .lock()
            .as_ref()
            .map(|r| r.commands.clone())
            .ok_or(ClientError::Offline)?;
        let (reply, answer) = oneshot::channel();
        commands
            .send(Command::Job(make(Some(reply))))
            .await
            .map_err(|_| ClientError::Offline)?;
        answer.await.map_err(|_| ClientError::Offline)?
    }

    // --- Admin (group-membership; device-identity: Invites) ---

    /// Admin: creates the group with this device as its only member
    /// (group-membership: Group creation).
    pub async fn create_group(&self) -> Result<(), ClientError> {
        self.job(|reply| Job::CreateGroup { reply }).await
    }

    /// Admin: adds all devices of `user` with one commit and its Welcome,
    /// after checking every device certificate against the user's identity
    /// key (pinned on first sight). Fails with
    /// [`ClientError::IdentityKeyChanged`] if the key differs from the pinned
    /// one.
    pub async fn add_user(&self, user: &UserId) -> Result<(), ClientError> {
        let user = user.clone();
        self.job(|reply| Job::AddUser { user, reply }).await
    }

    /// Admin: removes `user`: commits Removes of all their devices, then
    /// `Admin.RemoveUser`, which revokes the devices and the user's unused
    /// invites (group-membership: Removing a user).
    pub async fn remove_user(&self, user: &UserId) -> Result<(), ClientError> {
        let user = user.clone();
        self.job(|reply| Job::RemoveUser { user, reply }).await
    }

    /// Admin: a new invite for `username` (device-identity: Invites).
    pub async fn create_invite(&self, username: &str) -> Result<InviteInfo, ClientError> {
        use prost::Message as _;
        let session = self.session()?;
        let response = session
            .conn()
            .admin()
            .create_invite(session.request(pb::CreateInviteRequest {
                username: username.to_owned(),
            })?)
            .await
            .map_err(NetError::from)?
            .into_inner();
        let invite = response
            .invite
            .ok_or_else(|| NetError::Protocol("CreateInvite without invite".into()))?;
        let payload = response
            .payload
            .ok_or_else(|| NetError::Protocol("CreateInvite without payload".into()))?;
        let mut info = invite_info(&invite)?;
        info.payload = payload.encode_to_vec();
        Ok(info)
    }

    /// Admin: the unused invites.
    pub async fn list_invites(&self) -> Result<Vec<InviteInfo>, ClientError> {
        let session = self.session()?;
        let response = session
            .conn()
            .admin()
            .list_invites(session.request(pb::ListInvitesRequest {})?)
            .await
            .map_err(NetError::from)?
            .into_inner();
        response.invites.iter().map(invite_info).collect()
    }

    /// Admin: revokes an unused invite.
    pub async fn revoke_invite(&self, code: &str) -> Result<(), ClientError> {
        let session = self.session()?;
        session
            .conn()
            .admin()
            .revoke_invite(session.request(pb::RevokeInviteRequest {
                code: code.to_owned(),
            })?)
            .await
            .map_err(NetError::from)?;
        Ok(())
    }

    // --- Own devices (device-identity) ---

    /// Revokes another device of this user (device-identity: Device
    /// revocation) and removes it from the group.
    pub async fn revoke_device(&self, device: &DeviceId) -> Result<(), ClientError> {
        let session = self.session()?;
        session
            .conn()
            .devices()
            .revoke_device(session.request(pb::RevokeDeviceRequest {
                device_id: device.to_vec(),
            })?)
            .await
            .map_err(NetError::from)?;
        self.shared.store.remove_pending_link(device)?;
        let device = *device;
        self.job(|reply| Job::RemoveDevice { device, reply }).await
    }

    /// account-deletion: User deletes their own account. Calls the self-service
    /// `Devices.DeleteAccount` — which targets this caller's own user, since the
    /// request carries no user id — and on an OK response wipes this device with
    /// [`Client::reset`]. On any error nothing is wiped and the account stays
    /// usable (account-deletion: No wipe without server confirmation). The admin
    /// device never calls the RPC and gets [`ClientError::AdminCannotDelete`]
    /// (account-deletion: The admin cannot self-delete through the RPC); the
    /// server refuses the admin as the backstop.
    pub async fn delete_account(&self) -> Result<(), ClientError> {
        if self.is_admin()? {
            return Err(ClientError::AdminCannotDelete);
        }
        let session = self.session()?;
        session
            .conn()
            .devices()
            .delete_account(session.request(pb::DeleteAccountRequest {})?)
            .await
            .map_err(NetError::from)?;
        self.reset().await
    }

    /// client-runtime: Device reset. Returns this device to a clean no-account
    /// state: stops the engine and connection, forgets the session, wipes every
    /// local table (account, groups, DMs, message history, pending jobs and the
    /// OpenMLS group state), clears the media cache files and deletes the device's identity, auth and MLS
    /// signature keys and its saved session token from the key store. The
    /// storage key that seals the now-empty local database at rest is kept — it
    /// holds no account or identity material, and removing it without recreating
    /// the database would strand it (design D7). Afterwards [`Client::user_id`]
    /// is `None` and the app returns to onboarding. Used after a successful
    /// [`Client::delete_account`] (account-deletion: The initiating device wipes
    /// itself on success).
    pub async fn reset(&self) -> Result<(), ClientError> {
        use std::sync::atomic::Ordering::Relaxed;
        self.stop();
        *self.shared.session.lock() = None;
        self.shared.server_knows_dms.store(false, Relaxed);
        self.shared.server_knows_named_groups.store(false, Relaxed);
        self.shared.server_knows_presence.store(false, Relaxed);
        self.shared.max_video_blob_bytes.store(0, Relaxed);
        let store = Arc::clone(&self.shared.store);
        let key_store = Arc::clone(&self.shared.key_store);
        let media = self.shared.media.clone();
        blocking(move || {
            store.wipe()?;
            // The decrypted pictures, voice messages and clips are files outside
            // the database: they go too.
            media.clear()?;
            for name in [
                KeyName::IdentityKey,
                KeyName::DeviceAuthKey,
                KeyName::MlsSignatureKey,
                KeyName::SessionToken,
            ] {
                key_store.delete(name)?;
            }
            Ok(())
        })
        .await
    }

    /// Registers, replaces or (with `None`) clears this device's push token
    /// (device-identity: Push token registration). The app calls it on each
    /// connect and when the platform rotates the token (app-ui: Push
    /// notifications).
    pub async fn set_push_token(&self, token: Option<PushToken>) -> Result<(), ClientError> {
        let session = self.session()?;
        let push_token = token.map(|t| pb::PushToken {
            platform: match t.platform {
                PushPlatform::Fcm => pb::PushPlatform::Fcm,
                PushPlatform::Apns => pb::PushPlatform::Apns,
            } as i32,
            token: t.token,
        });
        session
            .conn()
            .devices()
            .update_push_token(session.request(pb::UpdatePushTokenRequest { push_token })?)
            .await
            .map_err(NetError::from)?;
        Ok(())
    }

    // --- Identity keys (key-directory: Clients verify the identity chain) ---

    /// The user accepted `user`'s new identity key in the UI (after an
    /// [`Event::IdentityKeyChanged`]): it becomes the pinned key and adding
    /// their devices works again. On admin devices this also adds the user's
    /// devices that wait for it (group-membership: Automatic add of new
    /// users). Returns `false` if no new key is pending.
    pub fn accept_identity_key(&self, user: &UserId) -> Result<bool, ClientError> {
        let id = user.to_string();
        let Some(pinned) = self.shared.store.pinned_identity_key(&id)? else {
            return Ok(false);
        };
        let Some(pending) = pinned.pending else {
            return Ok(false);
        };
        let accepted = self
            .shared
            .store
            .accept_identity_key(&id, &pending, self.now())?;
        if accepted
            && self.account()?.is_admin
            && let Some(running) = self.running.lock().as_ref()
        {
            // If the queue is full the next JoinNotice (at the next connect)
            // adds them instead.
            let _ = running.commands.try_send(Command::Job(Job::AddUser {
                user: user.clone(),
                reply: None,
            }));
        }
        Ok(accepted)
    }

    /// The owner acknowledges `user`'s changed safety number after an
    /// [`Event::IdentityKeyChanged`] (admin-recovery: Admin re-key forces peer
    /// re-trust). It promotes a key the admin-gated path kept waiting (as
    /// [`Client::accept_identity_key`] does, re-adding on admin devices) and
    /// clears a pending-re-trust mark left by the auto-repin path, so the
    /// identity is treated as verified again. Returns `true` if anything was
    /// acknowledged. Trust is withheld until this is called: no path accepts a
    /// changed key as verified on its own.
    pub fn acknowledge_identity_key(&self, user: &UserId) -> Result<bool, ClientError> {
        let promoted = self.accept_identity_key(user)?;
        let cleared = self
            .shared
            .store
            .acknowledge_retrust(&user.to_string(), self.now())?;
        Ok(promoted || cleared)
    }

    /// The users whose identity is unverified / pending re-trust: a changed key
    /// is waiting to be accepted, or the pinned key was auto-repinned and not
    /// yet acknowledged (admin-recovery: Admin re-key forces peer re-trust).
    pub fn identity_retrust_pending(&self) -> Result<Vec<UserId>, ClientError> {
        let ids = self.shared.store.identity_retrust_pending()?;
        Ok(ids.iter().filter_map(|id| UserId::parse(id)).collect())
    }

    // --- Messaging (message-content; client-runtime: Send status) ---

    /// Sends a text message (message-content: Text only). It is stored with
    /// its plaintext as sending first (client-runtime: Local message store)
    /// and sent as soon as the engine can, also after a restart; its status
    /// arrives as [`Event::SendStatus`]. Fails with [`ClientError::TooLong`]
    /// for text that would encrypt to more than the ciphertext cap.
    pub fn send_text(&self, text: &str) -> Result<MessageId, ClientError> {
        self.send_message(text, &[], None)
    }

    /// Sends a text message with `mentions` (UTF-8 byte ranges of `text`;
    /// message-content: Mentions), answering the stored chat message
    /// `reply_to` if set (message-content: Replies; client-runtime: Reply
    /// lookup), like [`Client::send_text`]. Fails with
    /// [`ClientError::InvalidMention`] for a range a receiver would drop,
    /// and with [`ClientError::NoSuchMessage`] when `reply_to` is not a
    /// stored chat message; nothing is stored then.
    pub fn send_message(
        &self,
        text: &str,
        mentions: &[crate::Mention],
        reply_to: Option<MessageId>,
    ) -> Result<MessageId, ClientError> {
        let reply = reply_to.map(|id| self.reply_ref(id)).transpose()?;
        let content = crate::messaging::encode_message(text, mentions, reply.as_ref(), self.now())
            .map_err(too_long)?;
        self.send_content(&content)
    }

    /// The `ReplyTo` for answering the stored chat message `id`.
    fn reply_ref(&self, id: MessageId) -> Result<crate::messaging::ReplyTo, ClientError> {
        use crate::messaging::Decoded;
        let stored = self
            .shared
            .store
            .message(id)?
            .ok_or(ClientError::NoSuchMessage)?;
        // The quote text plus what the original was, so a receiver without the
        // original renders the right preview (message-content: Reply quote
        // describes the original's kind): a text excerpt, a picture's caption +
        // ThumbHash (for a thumbnail), or a voice duration.
        let (quote, original_kind, original_duration_ms, original_thumbhash) =
            match crate::messaging::decode(&stored.content).body {
                Decoded::Text(text) => (text.text, ReplyOriginalKind::Text, 0, Vec::new()),
                // Replying to a picture quotes its caption (if any) and carries
                // its ThumbHash so the quote shows a thumbnail.
                Decoded::Image(image) => (
                    image.caption.unwrap_or_default(),
                    ReplyOriginalKind::Image,
                    0,
                    image.thumbhash,
                ),
                // A voice message has no text to quote; carry its duration.
                Decoded::Audio(audio) => (
                    String::new(),
                    ReplyOriginalKind::Audio,
                    audio.duration_ms,
                    Vec::new(),
                ),
                // Replying to a video quotes its caption (if any), as a picture
                // reply quotes its caption, and carries its first-frame ThumbHash
                // so the quote shows a thumbnail; `original_kind` carries the kind
                // so a receiver without the clip renders a video preview, never the
                // "needs a newer version" placeholder (message-content: Reply quote
                // describes the original's kind; Video caption and mentions).
                Decoded::Video(video) => (
                    video.caption.unwrap_or_default(),
                    ReplyOriginalKind::Video,
                    0,
                    video.thumbhash,
                ),
                // A body this core cannot decode has no kind to carry.
                Decoded::Unsupported => (String::new(), ReplyOriginalKind::Text, 0, Vec::new()),
                Decoded::Profile(_) | Decoded::GroupSettings { .. } | Decoded::Delete(_) => {
                    return Err(ClientError::NoSuchMessage);
                }
            };
        let user = UserId::parse(&stored.sender_user).ok_or(ClientError::NoSuchMessage)?;
        Ok(crate::messaging::ReplyTo {
            user,
            device: stored.sender_device,
            msg_id: stored.msg_id,
            quote,
            original_kind,
            original_duration_ms,
            original_thumbhash,
        })
    }

    /// The messages in the main group whose mention of this user (or
    /// everyone) is not seen yet, in chat order (client-runtime: Unseen
    /// mentions).
    pub fn unseen_mentions(&self) -> Result<Vec<MessageId>, ClientError> {
        Ok(self.shared.store.unseen_mentions()?)
    }

    /// [`Client::unseen_mentions`] in chat `chat` (change `dms`).
    pub fn unseen_mentions_in(&self, chat: ChatId) -> Result<Vec<MessageId>, ClientError> {
        Ok(self.shared.store.unseen_mentions_in(chat.0)?)
    }

    // --- Chats (direct-messages; app-ui: Chat list) ---

    /// Whether the server this device last connected to knows DMs (it lists
    /// protocol version 2); the app offers "Message" only then.
    pub fn dms_available(&self) -> bool {
        self.shared
            .server_knows_dms
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Whether the server this device last connected to knows named groups (it
    /// lists protocol version 3); the app offers "Create group" only then.
    pub fn named_groups_available(&self) -> bool {
        self.shared
            .server_knows_named_groups
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// The chats: the lobby first, then the named groups and the DMs, newest
    /// message first, closed ones last (app-ui: Chat list).
    pub fn chats(&self) -> Result<Vec<Chat>, ClientError> {
        let store = &self.shared.store;
        let summary = |id: ChatId, kind: ChatKind, group_id: &[u8]| -> Result<Chat, ClientError> {
            Ok(Chat {
                id,
                kind,
                unread: store.unread_in(id.0)?,
                unseen_mentions: u32::try_from(store.unseen_mentions_in(id.0)?.len())
                    .unwrap_or(u32::MAX),
                last_message_at: store.last_message_at(id.0)?,
                // notification-prefs: Mute a conversation. Per-conversation
                // marker; whole-app mute is shown globally, not per row.
                muted: store.is_conversation_muted(group_id)?,
            })
        };
        let mut chats = Vec::new();
        if let Some(main) = store.group()? {
            chats.push(summary(ChatId::MAIN, ChatKind::Lobby, &main.group_id)?);
        }
        let mut others = Vec::new();
        for dm in store.dms()? {
            let closed = dm.closed_at.is_some();
            if dm.named {
                others.push(summary(
                    ChatId(dm.chat),
                    ChatKind::Group {
                        name: dm.name,
                        closed,
                    },
                    &dm.group_id,
                )?);
                continue;
            }
            let Some(peer) = UserId::parse(&dm.peer) else {
                continue;
            };
            // A DM with a blocked peer is hidden from the chat list (moderation:
            // A blocked user is hidden); it reappears on unblock.
            if store.is_user_blocked(&peer.to_string())? {
                continue;
            }
            let waiting = !closed && !self.dm_has_peer(&dm.group_id, &peer)?;
            others.push(summary(
                ChatId(dm.chat),
                ChatKind::Dm {
                    peer,
                    waiting,
                    closed,
                },
                &dm.group_id,
            )?);
        }
        others.sort_by(|a, b| {
            let closed = |c: &Chat| {
                matches!(
                    c.kind,
                    ChatKind::Dm { closed: true, .. } | ChatKind::Group { closed: true, .. }
                )
            };
            closed(a)
                .cmp(&closed(b))
                .then(b.last_message_at.cmp(&a.last_message_at))
                .then(a.id.cmp(&b.id))
        });
        chats.extend(others);
        Ok(chats)
    }

    // --- Named groups (multi-group; admin only to create and moderate) ---

    /// Admin: creates a named group with `name` (if given) and adds the
    /// version 3 devices of `members`, which finishes in the background
    /// ([`Event::ChatsChanged`]); returns its chat. Fails with
    /// [`ClientError::NotAdmin`] on other devices and
    /// [`ClientError::GroupsUnavailable`] on a server below version 3.
    pub async fn create_named_group(
        &self,
        name: Option<&str>,
        members: &[UserId],
    ) -> Result<ChatId, ClientError> {
        let mut group_id = [0u8; 16];
        getrandom::fill(&mut group_id)
            .map_err(|e| ClientError::Mls(MlsError::new("randomness", e)))?;
        let (name, members) = (name.map(str::to_owned), members.to_vec());
        self.job(move |reply| Job::CreateNamedGroup {
            group_id: group_id.to_vec(),
            name,
            members,
            reply,
        })
        .await?;
        let chat = self
            .shared
            .store
            .dm_by_group(&group_id)?
            .ok_or(ClientError::NoSuchChat)?;
        Ok(ChatId(chat.chat))
    }

    fn named_group(&self, chat: ChatId) -> Result<crate::store::DmRow, ClientError> {
        self.shared
            .store
            .dm_by_chat(chat.0)?
            .filter(|row| row.named)
            .ok_or(ClientError::NoSuchChat)
    }

    /// Admin: adds the version 3 devices of `user` to the named group `chat`.
    pub async fn add_to_group(&self, chat: ChatId, user: &UserId) -> Result<(), ClientError> {
        let row = self.named_group(chat)?;
        let user = user.clone();
        self.job(move |reply| Job::AddToGroup {
            group_id: row.group_id,
            user,
            reply,
        })
        .await
    }

    /// Admin: removes all devices of `user` from the named group `chat`; they
    /// stay in the lobby.
    pub async fn remove_from_group(&self, chat: ChatId, user: &UserId) -> Result<(), ClientError> {
        let row = self.named_group(chat)?;
        let user = user.clone();
        self.job(move |reply| Job::RemoveFromGroup {
            group_id: row.group_id,
            user,
            reply,
            after: None,
        })
        .await
    }

    /// Admin: sets the name of the named group `chat` for every member
    /// (message-content: Group settings travel encrypted). The lobby's name is
    /// fixed: it is [`ClientError::NoSuchChat`] here.
    pub fn rename_group(&self, chat: ChatId, name: &str) -> Result<MessageId, ClientError> {
        if !self.account()?.is_admin {
            return Err(ClientError::NotAdmin);
        }
        let row = self.named_group(chat)?;
        if row.closed_at.is_some() {
            return Err(ClientError::ChatClosed);
        }
        let content =
            crate::messaging::encode_group_settings(Some(name), self.now()).map_err(too_long)?;
        self.shared.store.set_named_name(&row.group_id, name)?;
        let id = self.queue_content(chat.0, &content)?;
        self.shared.events.emit(Event::ChatsChanged);
        Ok(id)
    }

    /// Admin: closes the named group `chat` (`Admin.CloseGroup`). It becomes
    /// read-only for everyone; the lobby cannot be closed.
    pub async fn close_group(&self, chat: ChatId) -> Result<(), ClientError> {
        let row = self.named_group(chat)?;
        self.job(move |reply| Job::CloseGroup {
            group_id: row.group_id,
            reply,
        })
        .await
    }

    /// message-deletion: Remove a closed DM. Tells the server
    /// (`Dms.RemoveClosedDm`, so the DM leaves the user's other devices too),
    /// then deletes the DM, its stored messages and its read mark here. Nothing
    /// is deleted if the server call fails. [`ClientError::NoSuchChat`] unless
    /// `chat` is a closed DM; an open DM, a group and the lobby cannot be
    /// removed this way.
    pub async fn remove_closed_dm(&self, chat: ChatId) -> Result<(), ClientError> {
        let row = self
            .shared
            .store
            .dm_by_chat(chat.0)?
            .filter(|row| !row.named && row.closed_at.is_some())
            .ok_or(ClientError::NoSuchChat)?;
        self.job(move |reply| Job::RemoveClosedDm {
            group_id: row.group_id,
            reply,
        })
        .await
    }

    // --- Notification prefs (notification-prefs; change `mute-notifications`) ---

    /// Mutes or unmutes `target` (notification-prefs: Mute a conversation; Mute
    /// the whole app). Tells the server (`Prefs.SetMutePref`, so the preference
    /// reaches this user's other devices), then records it here. Nothing changes
    /// locally if the server call fails (an old server answers `UNIMPLEMENTED`,
    /// surfaced as [`ClientError`], and the conversation stays unmuted).
    /// [`ClientError::NoSuchChat`] for an unknown conversation.
    pub async fn set_mute_pref(&self, target: MuteTarget, muted: bool) -> Result<(), ClientError> {
        let (scope, group_id) = match target {
            MuteTarget::Conversation(chat) => {
                (pb::MuteScope::Conversation, self.group_id_of(chat)?)
            }
            MuteTarget::Global => (pb::MuteScope::Global, Vec::new()),
        };
        self.job(move |reply| Job::SetMutePref {
            scope,
            group_id,
            muted,
            reply,
        })
        .await
    }

    /// The user's mute preferences, as this device holds them
    /// (notification-prefs: Mute preference sync).
    pub fn mute_prefs(&self) -> Result<MutePrefs, ClientError> {
        let store = &self.shared.store;
        let mut muted_chats = Vec::new();
        for group_id in store.muted_group_ids()? {
            if let Some(chat) = self.chat_of_group(&group_id)? {
                muted_chats.push(chat);
            }
        }
        Ok(MutePrefs {
            muted_chats,
            muted_all: store.muted_all()?,
        })
    }

    // --- Presence (manual-presence; change `manual-presence`) ---

    /// Sets this user's own manual presence flag (manual-presence: Setting your
    /// status): records it on this device at once, then sends it to the server —
    /// now if connected to a server that knows presence, else on the next
    /// connect. The flag is durable and manual: it is unaffected by connection
    /// state and the server keeps it until the user changes it, distributing it
    /// to everyone who shares a conversation with this user.
    pub fn set_presence(&self, status: Presence) -> Result<(), ClientError> {
        self.shared.store.set_self_presence(status.to_protocol())?;
        if let Some(running) = self.running.lock().as_ref() {
            let _ = running.commands.try_send(Command::SetPresence(status));
        }
        Ok(())
    }

    /// This user's own manual presence flag, as this device holds it
    /// (manual-presence: Manual status, never inferred).
    pub fn my_presence(&self) -> Result<Presence, ClientError> {
        Ok(Presence::from_protocol(
            self.shared.store.self_presence()?.status,
        ))
    }

    /// The manual presence flag this device last learned for `user`, or `None`
    /// when it has learned none — the UI then shows no dot rather than guess a
    /// colour (manual-presence: Presence indicator on avatars).
    pub fn presence_of(&self, user: &UserId) -> Result<Option<Presence>, ClientError> {
        Ok(self
            .shared
            .store
            .peer_presence(&user.to_string())?
            .map(Presence::from_protocol))
    }

    /// Whether notifications for `chat` are silenced right now: whole-app mute,
    /// or this conversation muted individually (notification-prefs).
    pub fn is_muted(&self, chat: ChatId) -> Result<bool, ClientError> {
        if self.shared.store.muted_all()? {
            return Ok(true);
        }
        match self.group_id_of(chat) {
            Ok(group_id) => Ok(self.shared.store.is_conversation_muted(&group_id)?),
            // An unknown or not-yet-joined conversation cannot be muted.
            Err(ClientError::NoSuchChat | ClientError::NoGroup) => Ok(false),
            Err(error) => Err(error),
        }
    }

    // --- Local notifications (change `smarter-notifications`) ---

    /// What to show as this device's notifications right now: per conversation
    /// with unread messages its type and unread count, plus the pending
    /// admin-events (local-notifications: The device builds the notification
    /// locally). Metadata only: no sender, body or caption exists in the
    /// result. Recomputed from the core's unread state on every call, so a
    /// missed, collapsed or duplicated wake cannot change the count
    /// (local-notifications: Unread count, not push count). Muted and closed
    /// conversations are left out, and whole-app mute silences everything
    /// (the local gate for `mute-notifications`).
    pub fn notification_summary(&self) -> Result<NotificationSummary, ClientError> {
        let store = &self.shared.store;
        let chats = self.chats()?;
        let events = store
            .admin_events()?
            .into_iter()
            .map(|(id, kind, group_id)| crate::notification::StoredEvent { id, kind, group_id })
            .collect();
        Ok(crate::notification::build(
            &chats,
            store.muted_all()?,
            events,
            |group_id| self.chat_of_group(group_id).ok().flatten(),
        ))
    }

    /// The type of the conversation whose wire `group_id` is `group_id`, from the
    /// device's local record alone: no message is read or decrypted
    /// (local-notifications: Conversation type without decryption). `None` for a
    /// group this device does not know.
    pub fn conversation_kind_of_group(
        &self,
        group_id: &[u8],
    ) -> Result<Option<ConversationKind>, ClientError> {
        let store = &self.shared.store;
        if store.is_main(group_id)? {
            return Ok(Some(ConversationKind::Lobby));
        }
        Ok(store.dm_by_group(group_id)?.map(|dm| {
            if dm.named {
                ConversationKind::NamedGroup
            } else {
                ConversationKind::Dm
            }
        }))
    }

    /// The wire `group_id` of `chat` (the id a push-woken device sees on an
    /// `Envelope`); [`NoSuchChat`](ClientError::NoSuchChat) for an unknown one.
    pub fn chat_group_id(&self, chat: ChatId) -> Result<Vec<u8>, ClientError> {
        self.group_id_of(chat)
    }

    /// Forgets the pending admin-events up to `up_to` (the largest
    /// [`AdminEvent::id`] the app notified for, or the user is looking at the
    /// app and needs no notification for them).
    pub fn clear_admin_events(&self, up_to: i64) -> Result<(), ClientError> {
        self.shared.store.clear_admin_events(up_to)?;
        Ok(())
    }

    // --- Moderation (block + report; change `report-and-block`) ---

    /// Blocks `user` on this device (moderation: Block a user): records the id
    /// in the local block list so the display and notification paths skip the
    /// user's messages and DM. Local only — it writes no RPC and no server
    /// marker, and never travels to the user's other devices or to the blocked
    /// user (design D1). Idempotent; blocking never changes group membership or
    /// MLS state.
    pub fn block_user(&self, user: &UserId) -> Result<(), ClientError> {
        self.shared.store.block_user(&user.to_string())?;
        Ok(())
    }

    /// Unblocks `user` on this device (moderation: Block a user — Unblock): the
    /// user's stored messages and any DM are shown again. Idempotent.
    pub fn unblock_user(&self, user: &UserId) -> Result<(), ClientError> {
        self.shared.store.unblock_user(&user.to_string())?;
        Ok(())
    }

    /// Whether this device has blocked `user`.
    pub fn is_blocked(&self, user: &UserId) -> Result<bool, ClientError> {
        Ok(self.shared.store.is_user_blocked(&user.to_string())?)
    }

    /// The users this device has blocked, sorted (moderation: Block a user).
    pub fn blocked_users(&self) -> Result<Vec<UserId>, ClientError> {
        Ok(self
            .shared
            .store
            .blocked_users()?
            .iter()
            .filter_map(|id| crate::messaging::parse_user(id))
            .collect())
    }

    /// Reports `target` to the admin (moderation: Report a message; Report a DM
    /// message; Report a user) by sending a report `Content.Text` over a DM with
    /// the admin, reusing the existing DM path. A group/lobby message report
    /// carries a reference (reported user + group id + wire `msg_id`); a DM
    /// message report quotes the offending text (reply-quote rules); a user
    /// report carries only the reported user and the reason. No new wire message
    /// is added, and no DM key or ciphertext leaves this device.
    pub async fn report(&self, target: ReportTarget, reason: &str) -> Result<(), ClientError> {
        let report = self.compose_report(target, reason)?;
        let admin = self.admin_user()?.ok_or(ClientError::NoGroup)?;
        let text = crate::moderation::compose(&report);
        let chat = self.open_dm(&admin).await?;
        self.send_message_in(chat, &text, &[], None)?;
        Ok(())
    }

    /// Builds the [`Report`] for `target`, reading the reported user from the
    /// stored message's authenticated sender and referencing a group/lobby
    /// message or quoting a DM one.
    fn compose_report(&self, target: ReportTarget, reason: &str) -> Result<Report, ClientError> {
        let reason = reason.to_string();
        match target {
            ReportTarget::User(user) => Ok(Report {
                reported: user,
                reason,
                kind: ReportKind::User,
            }),
            ReportTarget::Message(id) => {
                let stored = self
                    .shared
                    .store
                    .message(id)?
                    .ok_or(ClientError::NoSuchMessage)?;
                let reported = crate::messaging::parse_user(&stored.sender_user)
                    .ok_or(ClientError::NoSuchMessage)?;
                // A DM chat (not the lobby, not a named group) means the admin
                // is not a member, so the report must quote the content.
                let is_dm = match self.shared.store.dm_by_chat(stored.chat)? {
                    Some(dm) => !dm.named,
                    None => false,
                };
                let kind = if is_dm {
                    ReportKind::Dm {
                        quote: protocol::message_content::reply_quote(&quotable_text(
                            &stored.content,
                        )),
                    }
                } else {
                    ReportKind::Group {
                        group_id: self.group_id_of(ChatId(stored.chat))?,
                        msg_id: stored.msg_id.to_vec(),
                    }
                };
                Ok(Report {
                    reported,
                    reason,
                    kind,
                })
            }
        }
    }

    /// Parses a report `Content.Text` the admin received (moderation: A report
    /// reaches the admin), or `None` for an ordinary message. The admin's app
    /// calls it to present a report distinctly from a DM.
    pub fn parse_report(&self, text: &str) -> Option<Report> {
        crate::moderation::parse(text)
    }

    /// The chat `group_id` belongs to, or `None` for an unknown group.
    fn chat_of_group(&self, group_id: &[u8]) -> Result<Option<ChatId>, ClientError> {
        let store = &self.shared.store;
        if store.is_main(group_id)? {
            return Ok(Some(ChatId::MAIN));
        }
        Ok(store.dm_by_group(group_id)?.map(|dm| ChatId(dm.chat)))
    }

    /// The member devices of the named group `chat`, as this device sees
    /// them (the admin's member picker); empty once the group is closed.
    pub fn group_members(&self, chat: ChatId) -> Result<Vec<Member>, ClientError> {
        let row = self.named_group(chat)?;
        let store = &self.shared.store;
        let group = store.transaction(|| -> Result<_, ClientError> {
            Ok(crate::mls::group::load(store.mls(), &row.group_id)?)
        })?;
        let Some(group) = group else {
            return Ok(Vec::new());
        };
        let mut members: Vec<Member> = verify::members(&group)
            .into_iter()
            .map(|(_, claim)| Member {
                user: claim.claimed.user,
                device: claim.claimed.device_id,
            })
            .collect();
        members.sort();
        Ok(members)
    }

    /// Whether one of `peer`'s devices is in the DM `group_id` on this
    /// device.
    fn dm_has_peer(&self, group_id: &[u8], peer: &UserId) -> Result<bool, ClientError> {
        let store = &self.shared.store;
        let group = store.transaction(|| -> Result<_, ClientError> {
            Ok(crate::mls::group::load(store.mls(), group_id)?)
        })?;
        Ok(group.is_some_and(|g| !crate::membership::leaves_of_user(&g, peer).is_empty()))
    }

    /// The open DM with `user`, opening it first if there is none
    /// (direct-messages: Opening a DM). It can be written to at once; its
    /// messages go out once one of the peer's devices is in it.
    pub async fn open_dm(&self, user: &UserId) -> Result<ChatId, ClientError> {
        let peer = user.to_string();
        if let Some(dm) = self.shared.store.open_dm_with(&peer)? {
            return Ok(ChatId(dm.chat));
        }
        let wanted = user.clone();
        self.job(|reply| Job::OpenDm {
            peer: wanted,
            reply,
        })
        .await?;
        self.shared
            .store
            .open_dm_with(&peer)?
            .map(|dm| ChatId(dm.chat))
            .ok_or(ClientError::NoSuchChat)
    }

    /// The peer of DM `chat`; `None` for the main group.
    pub fn dm_peer(&self, chat: ChatId) -> Result<Option<UserId>, ClientError> {
        if chat.is_main() {
            return Ok(None);
        }
        let dm = self
            .shared
            .store
            .dm_by_chat(chat.0)?
            .ok_or(ClientError::NoSuchChat)?;
        Ok(UserId::parse(&dm.peer))
    }

    /// The user has seen chat `chat` up to its newest message (app-ui: Chat
    /// list, the unread mark).
    pub fn mark_chat_read(&self, chat: ChatId) -> Result<(), ClientError> {
        let newest = self
            .shared
            .store
            .history_in(chat.0, 1)?
            .last()
            .map(|m| m.id.0);
        if let Some(up_to) = newest {
            self.shared.store.mark_chat_read(chat.0, up_to)?;
        }
        Ok(())
    }

    /// The read mark of chat `chat`: the local id of the newest message read
    /// last, or [`MessageId`] 0 when it was never read (app-ui: the unread
    /// divider's snapshot). Only reads; never advances the mark.
    pub fn read_mark(&self, chat: ChatId) -> Result<MessageId, ClientError> {
        Ok(MessageId(self.shared.store.chat_read_mark(chat.0)?))
    }

    /// [`Client::send_message`] in chat `chat` (change `dms`). Fails with
    /// [`ClientError::NoSuchChat`] or [`ClientError::ChatClosed`] for a DM it
    /// cannot write to.
    pub fn send_message_in(
        &self,
        chat: ChatId,
        text: &str,
        mentions: &[crate::Mention],
        reply_to: Option<MessageId>,
    ) -> Result<MessageId, ClientError> {
        if chat.is_main() {
            return self.send_message(text, mentions, reply_to);
        }
        let dm = self
            .shared
            .store
            .dm_by_chat(chat.0)?
            .ok_or(ClientError::NoSuchChat)?;
        if dm.closed_at.is_some() {
            return Err(ClientError::ChatClosed);
        }
        let reply = reply_to.map(|id| self.reply_ref(id)).transpose()?;
        let content = crate::messaging::encode_message(text, mentions, reply.as_ref(), self.now())
            .map_err(too_long)?;
        self.queue_content(chat.0, &content)
    }

    // --- Deleting (message-deletion) ---

    /// Deletes the stored chat message `id` in chat `chat` for everyone
    /// (message-deletion: Who may delete). In the lobby and named groups only an
    /// admin device may delete, any message; in a DM only the user's own
    /// message. It queues a superseding `Content.Delete`, sent like any message;
    /// clients show a tombstone, and the server drops the target's undelivered
    /// copies. A media delete also purges the stored blob once the delete is
    /// accepted. Returns the local id of the queued delete. Fails with
    /// [`ClientError::CannotDelete`] when this device may not delete it,
    /// [`ClientError::NoSuchMessage`] when there is no such chat message in
    /// `chat`, or [`ClientError::ChatClosed`] for a closed DM.
    pub fn delete_message(&self, chat: ChatId, id: MessageId) -> Result<MessageId, ClientError> {
        use crate::messaging::Decoded;
        let account = self.account()?;
        let stored = self
            .shared
            .store
            .message(id)?
            .ok_or(ClientError::NoSuchMessage)?;
        // It must be a real, not-yet-deleted chat message in this chat.
        if stored.chat != chat.0 || stored.deleted {
            return Err(ClientError::NoSuchMessage);
        }
        if matches!(
            crate::messaging::decode(&stored.content).body,
            Decoded::Profile(_) | Decoded::GroupSettings { .. } | Decoded::Delete(_)
        ) {
            return Err(ClientError::NoSuchMessage);
        }
        let own_user = UserId::new(account.username.clone(), account.server_id.clone());
        // Who may delete: an admin in the lobby/named groups; the sender in a DM.
        let is_group = if chat.is_main() {
            true
        } else {
            let dm = self
                .shared
                .store
                .dm_by_chat(chat.0)?
                .ok_or(ClientError::NoSuchChat)?;
            if dm.closed_at.is_some() {
                return Err(ClientError::ChatClosed);
            }
            dm.named
        };
        let target_user = UserId::parse(&stored.sender_user).ok_or(ClientError::NoSuchMessage)?;
        if is_group {
            if !account.is_admin {
                return Err(ClientError::CannotDelete);
            }
        } else if target_user != own_user {
            return Err(ClientError::CannotDelete);
        }
        let content = crate::messaging::encode_delete(
            &crate::messaging::DeleteContent {
                target_user,
                target_device: stored.sender_device,
                target_msg_id: stored.msg_id,
            },
            self.now(),
        )
        .map_err(too_long)?;
        self.queue_content(chat.0, &content)
    }

    /// message-deletion: Receiver may reveal in a DM. The original of the
    /// deleted DM message `id` in chat `chat`, if this device still holds it.
    /// Only a DM tombstone can be revealed; the lobby and named groups offer no
    /// reveal. Fails with [`ClientError::NoSuchMessage`] when `id` is not a
    /// revealable DM tombstone in `chat`.
    pub fn reveal(&self, chat: ChatId, id: MessageId) -> Result<Message, ClientError> {
        let account = self.account()?;
        let stored = self
            .shared
            .store
            .message(id)?
            .ok_or(ClientError::NoSuchMessage)?;
        if stored.chat != chat.0 || !stored.deleted {
            return Err(ClientError::NoSuchMessage);
        }
        let is_dm = !chat.is_main()
            && self
                .shared
                .store
                .dm_by_chat(chat.0)?
                .map(|dm| !dm.named)
                .unwrap_or(false);
        if !is_dm {
            return Err(ClientError::NoSuchMessage);
        }
        let own_user = UserId::new(account.username, account.server_id);
        crate::messaging::revealed_message(&self.shared.store, &stored, &own_user)
            .ok_or(ClientError::NoSuchMessage)
    }

    // --- Images (message-content: Image messages; client-runtime: Image
    // encode pipeline, Blob crypto and send order, Image download and cache,
    // Expired media) ---

    /// Sends a picked image to the main group: [`Client::send_image_in`].
    pub async fn send_image(
        &self,
        picked: &[u8],
        caption: Option<&str>,
        mentions: &[crate::Mention],
    ) -> Result<MessageId, ClientError> {
        self.send_image_in(ChatId::MAIN, picked, caption, mentions)
            .await
    }

    /// Sends an image with an optional caption (and `mentions` of it, UTF-8
    /// byte ranges) to chat `chat`. `picked` is the file as the picker gave it
    /// (JPEG, PNG, GIF or WebP): the core turns it into a WebP of at most
    /// 2048 px and 2 MB with no metadata and keeps it in its media cache.
    ///
    /// The message is stored and queued at once, like text, connected or not.
    /// When the outbox reaches it with a connection it encrypts the picture
    /// under a fresh key, uploads the blob and only then sends the message
    /// (client-runtime: Blob crypto and send order), so a receiver never sees
    /// an image whose blob is missing. It shows as sending until then.
    pub async fn send_image_in(
        &self,
        chat: ChatId,
        picked: &[u8],
        caption: Option<&str>,
        mentions: &[crate::Mention],
    ) -> Result<MessageId, ClientError> {
        self.account()?;
        self.check_can_queue(chat)?;
        let picked = picked.to_vec();
        let encoded =
            blocking(move || Ok(crate::media::encode::encode(&picked).map_err(MediaError::from)?))
                .await?;
        self.queue_encoded(chat, encoded, caption, mentions).await
    }

    /// Sends a subject cutout as a sticker to the main group:
    /// [`Client::send_sticker_in`].
    pub async fn send_sticker(
        &self,
        cutout: crate::media::RgbaCutout,
    ) -> Result<MessageId, ClientError> {
        self.send_sticker_in(ChatId::MAIN, cutout).await
    }

    /// Sends a sticker — a transparent-background subject cutout — to chat
    /// `chat` (photo-sticker: Sticker is an image; Sticker styling and encode).
    /// `cutout` is the straight-alpha RGBA a `SegmentationSource` produced on the
    /// device; the core trims it, encodes WebP preserving the alpha and sends it
    /// down the unchanged image blob path (upload-then-`APPLICATION` order,
    /// cache, ack-delete, expiry, resend), so a sticker is an ordinary `Image`
    /// on the wire. The native side encodes nothing and never sees the key.
    ///
    /// Like an image it is queued at once, connected or not, and shows as
    /// sending until the blob is uploaded and the message accepted. A sticker
    /// carries no caption or mentions.
    pub async fn send_sticker_in(
        &self,
        chat: ChatId,
        cutout: crate::media::RgbaCutout,
    ) -> Result<MessageId, ClientError> {
        self.account()?;
        self.check_can_queue(chat)?;
        let encoded = blocking(move || {
            Ok(crate::media::encode::encode_cutout(&cutout).map_err(MediaError::from)?)
        })
        .await?;
        self.queue_encoded(chat, encoded, None, &[]).await
    }

    /// Resends the image of the own message `id` as a new message with a fresh
    /// blob, key and `msg_id`, from the picture this device kept: for when the
    /// first blob expired on the server (client-runtime: Expired media). Fails with
    /// [`ClientError::MediaNotOnDevice`] when the picture is no longer cached.
    pub async fn resend_image(&self, id: MessageId) -> Result<MessageId, ClientError> {
        use crate::messaging::Decoded;
        self.account()?;
        let stored = self
            .shared
            .store
            .message(id)?
            .ok_or(ClientError::NoSuchMessage)?;
        let Decoded::Image(image) = crate::messaging::decode(&stored.content).body else {
            return Err(ClientError::NoSuchMessage);
        };
        if !stored.outgoing {
            return Err(ClientError::NoSuchMessage);
        }
        let chat = ChatId(stored.chat);
        self.check_can_queue(chat)?;
        let webp = self
            .cached_webp(&image.blob_id)
            .await?
            .ok_or(ClientError::MediaNotOnDevice)?;
        let encoded = crate::media::EncodedImage {
            webp,
            width: image.width,
            height: image.height,
            thumbhash: image.thumbhash,
        };
        // Drop the old failed message so its bubble does not linger beside the
        // resend (local only; it was never accepted).
        self.shared.store.delete_failed_own(id)?;
        self.queue_encoded(chat, encoded, image.caption.as_deref(), &image.mentions)
            .await
    }

    /// Caches `image`, then stores its message as waiting for its blob and
    /// wakes the outbox, which uploads the blob and sends the message.
    async fn queue_encoded(
        &self,
        chat: ChatId,
        image: crate::media::EncodedImage,
        caption: Option<&str>,
        mentions: &[crate::Mention],
    ) -> Result<MessageId, ClientError> {
        use protocol::blob_store::{BLOB_NONCE_LEN, BLOB_TAG_LEN};
        let (blob_id, blob_key) = crate::media::crypto::fresh().map_err(MediaError::from)?;
        let content = crate::messaging::ImageContent {
            blob_id,
            blob_key,
            width: image.width,
            height: image.height,
            thumbhash: image.thumbhash,
            // The blob is the nonce, the picture and the tag.
            blob_size: (image.webp.len() + BLOB_NONCE_LEN + BLOB_TAG_LEN) as u64,
            caption: caption.map(str::to_owned),
            mentions: mentions.to_vec(),
        };
        // A bad caption or mention is refused before anything is kept.
        let encoded = crate::messaging::encode_image(&content, self.now()).map_err(too_long)?;
        // The sender cannot decrypt its own message, so it keeps the picture;
        // the outbox encrypts and uploads it from here.
        self.cache_webp(&blob_id, image.webp, true).await?;
        match self.queue_content_with(chat.0, &encoded, true) {
            Ok(id) => Ok(id),
            Err(error) => {
                let store = Arc::clone(&self.shared.store);
                let cache = self.shared.media.clone();
                let _ = blocking(move || Ok(cache.remove(&store, &blob_id)?)).await;
                Err(error)
            }
        }
    }

    // --- Voice messages (message-content: Audio messages; audio-messages:
    // Recording and encoding; the blob send order, download, cache and expiry
    // are the image ones) ---

    /// Sends a recorded voice message to the main group: [`Client::send_audio_in`].
    pub async fn send_audio(
        &self,
        wav: &[u8],
        reply_to: Option<MessageId>,
    ) -> Result<MessageId, ClientError> {
        self.send_audio_in(ChatId::MAIN, wav, reply_to).await
    }

    /// Sends a voice message to chat `chat`. `wav` is the file the recorder wrote
    /// (16-bit PCM, mono or stereo): the core turns it into OGG/Opus mono at the
    /// voice bitrate, refuses one longer than two minutes, computes the waveform
    /// and keeps the clip in its media cache.
    ///
    /// The message is stored and queued at once, like an image, connected or
    /// not; the outbox uploads the blob and then sends it (client-runtime: Blob
    /// crypto and send order). It shows as sending until then.
    pub async fn send_audio_in(
        &self,
        chat: ChatId,
        wav: &[u8],
        reply_to: Option<MessageId>,
    ) -> Result<MessageId, ClientError> {
        self.account()?;
        self.check_can_queue(chat)?;
        // Resolve the answered message before the slow encode, so a bad
        // reply-to fails fast and stores nothing (message-content: Voice replies).
        let reply = reply_to.map(|id| self.reply_ref(id)).transpose()?;
        let wav = wav.to_vec();
        let encoded =
            blocking(move || Ok(crate::media::audio::encode(&wav).map_err(MediaError::from)?))
                .await?;
        self.queue_encoded_audio(chat, encoded, reply).await
    }

    /// Resends the clip of the own voice message `id` as a new message with a
    /// fresh blob, key and `msg_id`, from the clip this device kept (client-runtime:
    /// Expired media). Fails with [`ClientError::MediaNotOnDevice`] when the clip
    /// is no longer cached.
    pub async fn resend_audio(&self, id: MessageId) -> Result<MessageId, ClientError> {
        use crate::messaging::Decoded;
        self.account()?;
        let stored = self
            .shared
            .store
            .message(id)?
            .ok_or(ClientError::NoSuchMessage)?;
        let Decoded::Audio(audio) = crate::messaging::decode(&stored.content).body else {
            return Err(ClientError::NoSuchMessage);
        };
        if !stored.outgoing {
            return Err(ClientError::NoSuchMessage);
        }
        let chat = ChatId(stored.chat);
        self.check_can_queue(chat)?;
        let path = self
            .cached_audio_path(&audio.blob_id)
            .await?
            .ok_or(ClientError::MediaNotOnDevice)?;
        let reply = audio.reply.clone();
        let ogg = blocking(move || Ok(std::fs::read(path).map_err(MediaError::from)?)).await?;
        let encoded = crate::media::EncodedAudio {
            ogg,
            duration_ms: audio.duration_ms,
            waveform: audio.waveform,
        };
        // Drop the old failed message so its bubble does not linger beside the
        // resend (it was never accepted; local only). A sending/accepted one is
        // left untouched.
        self.shared.store.delete_failed_own(id)?;
        // A resent voice message keeps what the original answered, if anything.
        self.queue_encoded_audio(chat, encoded, reply).await
    }

    /// Caches `audio`, then stores its message as waiting for its blob and wakes
    /// the outbox, which uploads the blob and sends the message.
    async fn queue_encoded_audio(
        &self,
        chat: ChatId,
        audio: crate::media::EncodedAudio,
        reply: Option<crate::messaging::ReplyTo>,
    ) -> Result<MessageId, ClientError> {
        use protocol::blob_store::{BLOB_NONCE_LEN, BLOB_TAG_LEN};
        let (blob_id, blob_key) = crate::media::crypto::fresh().map_err(MediaError::from)?;
        let content = crate::messaging::AudioContent {
            blob_id,
            blob_key,
            // The blob is the nonce, the clip and the tag.
            blob_size: (audio.ogg.len() + BLOB_NONCE_LEN + BLOB_TAG_LEN) as u64,
            duration_ms: audio.duration_ms,
            waveform: audio.waveform,
            reply,
        };
        let encoded = crate::messaging::encode_audio(&content, self.now()).map_err(too_long)?;
        // The sender cannot decrypt its own message, so it keeps the clip; the
        // outbox encrypts and uploads it from here.
        self.cache_blob(crate::media::MediaKind::Audio, &blob_id, audio.ogg, true)
            .await?;
        match self.queue_content_with(chat.0, &encoded, true) {
            Ok(id) => Ok(id),
            Err(error) => {
                let store = Arc::clone(&self.shared.store);
                let cache = self.shared.media.clone();
                let _ = blocking(move || Ok(cache.remove(&store, &blob_id)?)).await;
                Err(error)
            }
        }
    }

    // --- Video messages (message-content: Video messages; video-messages:
    // Video normalization in a native plugin). The core does the blob crypto
    // only; the already-normalized H.264/MP4 arrives from the app's native
    // plugin. The blob send order, download, cache, ack-delete, expiry and
    // resend are the image/audio ones. ---

    /// The advertised maximum blob size to enforce a video against
    /// (video-messages: Config-driven limit): what the server last advertised, or
    /// the protocol default before the first connect.
    fn max_video_blob_bytes(&self) -> u64 {
        let advertised = self
            .shared
            .max_video_blob_bytes
            .load(std::sync::atomic::Ordering::Relaxed);
        if advertised == 0 {
            protocol::video::DEFAULT_MAX_VIDEO_BYTES as u64
        } else {
            advertised
        }
    }

    /// Sends a normalized video to the main group: [`Client::send_video_in`].
    #[allow(clippy::too_many_arguments)]
    pub async fn send_video(
        &self,
        mp4: &[u8],
        duration_ms: u32,
        width: u32,
        height: u32,
        first_frame_rgba: &[u8],
        frame_width: u32,
        frame_height: u32,
        caption: Option<&str>,
        mentions: &[crate::Mention],
    ) -> Result<MessageId, ClientError> {
        self.send_video_in(
            ChatId::MAIN,
            mp4,
            duration_ms,
            width,
            height,
            first_frame_rgba,
            frame_width,
            frame_height,
            caption,
            mentions,
        )
        .await
    }

    /// Sends a video to chat `chat`. `mp4` is the already-normalized clip the
    /// native plugin produced (H.264/MP4 + AAC at the 480p/~1 Mbps target, trimmed
    /// to ≤ 30 s); the core does not transcode (video-messages: Video
    /// normalization in a native plugin). `first_frame_rgba` is the clip's first
    /// frame as RGBA (`frame_width` x `frame_height`), from which the core
    /// computes the body's ThumbHash — pass an empty slice to send no blur.
    ///
    /// The core refuses a clip longer than 30 s ([`ClientError::VideoTooLong`]),
    /// one whose encrypted blob would exceed the server's advertised maximum blob
    /// size ([`ClientError::VideoTooLarge`]), and bytes that are not an MP4
    /// ([`ClientError::NotAVideo`]). Otherwise it encrypts the clip under a fresh
    /// key, keeps it in its media cache, and queues the message; the outbox
    /// uploads the blob and only then sends it (client-runtime: Blob crypto and
    /// send order), so a receiver never sees a video whose blob is missing. It
    /// shows as sending until then. Video is allowed in the lobby, named groups
    /// and DMs, like an image.
    #[allow(clippy::too_many_arguments)]
    pub async fn send_video_in(
        &self,
        chat: ChatId,
        mp4: &[u8],
        duration_ms: u32,
        width: u32,
        height: u32,
        first_frame_rgba: &[u8],
        frame_width: u32,
        frame_height: u32,
        caption: Option<&str>,
        mentions: &[crate::Mention],
    ) -> Result<MessageId, ClientError> {
        self.account()?;
        self.check_can_queue(chat)?;
        if duration_ms == 0 || duration_ms > protocol::video::MAX_VIDEO_DURATION_MS {
            return Err(ClientError::VideoTooLong);
        }
        if width == 0 || height == 0 || !crate::media::video::looks_like_mp4(mp4) {
            return Err(ClientError::NotAVideo);
        }
        if protocol::video::blob_size(mp4.len()) > self.max_video_blob_bytes() {
            return Err(ClientError::VideoTooLarge);
        }
        let thumbhash =
            crate::media::video::first_frame_thumbhash(first_frame_rgba, frame_width, frame_height)
                .unwrap_or_default();
        self.queue_encoded_video(
            chat,
            mp4.to_vec(),
            duration_ms,
            width,
            height,
            thumbhash,
            caption,
            mentions,
        )
        .await
    }

    /// Resends the clip of the own video message `id` as a new message with a
    /// fresh blob, key and `msg_id`, from the clip this device kept
    /// (client-runtime: Expired media). Fails with
    /// [`ClientError::MediaNotOnDevice`] when the clip is no longer cached.
    pub async fn resend_video(&self, id: MessageId) -> Result<MessageId, ClientError> {
        use crate::messaging::Decoded;
        self.account()?;
        let stored = self
            .shared
            .store
            .message(id)?
            .ok_or(ClientError::NoSuchMessage)?;
        let Decoded::Video(video) = crate::messaging::decode(&stored.content).body else {
            return Err(ClientError::NoSuchMessage);
        };
        if !stored.outgoing {
            return Err(ClientError::NoSuchMessage);
        }
        let chat = ChatId(stored.chat);
        self.check_can_queue(chat)?;
        // The advertised cap may have dropped since the first send; refuse a
        // resend the server would now reject.
        if video.blob_size > self.max_video_blob_bytes() {
            return Err(ClientError::VideoTooLarge);
        }
        let path = self
            .cached_video_path(&video.blob_id)
            .await?
            .ok_or(ClientError::MediaNotOnDevice)?;
        let mp4 = blocking(move || Ok(std::fs::read(path).map_err(MediaError::from)?)).await?;
        // The resend keeps the original's ThumbHash, dimensions and caption.
        self.queue_encoded_video(
            chat,
            mp4,
            video.duration_ms,
            video.width,
            video.height,
            video.thumbhash,
            video.caption.as_deref(),
            &video.mentions,
        )
        .await
    }

    /// Caches `mp4`, then stores its message as waiting for its blob and wakes the
    /// outbox, which uploads the blob and sends the message.
    #[allow(clippy::too_many_arguments)]
    async fn queue_encoded_video(
        &self,
        chat: ChatId,
        mp4: Vec<u8>,
        duration_ms: u32,
        width: u32,
        height: u32,
        thumbhash: Vec<u8>,
        caption: Option<&str>,
        mentions: &[crate::Mention],
    ) -> Result<MessageId, ClientError> {
        let (blob_id, blob_key) = crate::media::crypto::fresh().map_err(MediaError::from)?;
        let content = crate::messaging::VideoContent {
            blob_id,
            blob_key,
            blob_size: protocol::video::blob_size(mp4.len()),
            duration_ms,
            width,
            height,
            thumbhash,
            caption: caption.map(str::to_owned),
            mentions: mentions.to_vec(),
        };
        let encoded = crate::messaging::encode_video(&content, self.now()).map_err(too_long)?;
        // The sender cannot decrypt its own message, so it keeps the clip; the
        // outbox encrypts and uploads it from here.
        self.cache_blob(crate::media::MediaKind::Video, &blob_id, mp4, true)
            .await?;
        match self.queue_content_with(chat.0, &encoded, true) {
            Ok(id) => Ok(id),
            Err(error) => {
                let store = Arc::clone(&self.shared.store);
                let cache = self.shared.media.clone();
                let _ = blocking(move || Ok(cache.remove(&store, &blob_id)?)).await;
                Err(error)
            }
        }
    }

    /// The clip of video message `id` as the path of an MP4 file in this device's
    /// media cache, for a player to open: from the cache if it is there, else
    /// downloaded, decrypted and cached, and the blob acked so the server can
    /// delete it once everyone has it. The file stays in the core's media
    /// directory; Dart never copies it. Fails with [`ClientError::MediaExpired`]
    /// when the server no longer has it, and with [`ClientError::Offline`] when it
    /// must download and is not connected.
    pub async fn download_video(&self, id: MessageId) -> Result<std::path::PathBuf, ClientError> {
        use crate::messaging::Decoded;
        let stored = self
            .shared
            .store
            .message(id)?
            .ok_or(ClientError::NoSuchMessage)?;
        let Decoded::Video(video) = crate::messaging::decode(&stored.content).body else {
            return Err(ClientError::NoSuchMessage);
        };
        if let Some(path) = self.cached_video_path(&video.blob_id).await? {
            return Ok(path);
        }
        if self.shared.store.is_media_expired(&video.blob_id)? {
            return Err(ClientError::MediaExpired);
        }
        let blob = self.fetch_blob(ChatId(stored.chat), &video.blob_id).await?;
        let (key, blob_id) = (video.blob_key, video.blob_id);
        let mp4 = blocking(move || {
            let mp4 =
                crate::media::crypto::decrypt(&key, &blob_id, &blob).map_err(MediaError::from)?;
            // Only a clip the core recognizes as MP4 is cached and played.
            if !crate::media::video::looks_like_mp4(&mp4) {
                return Err(ClientError::NotAVideo);
            }
            Ok(mp4)
        })
        .await?;
        self.cache_blob(crate::media::MediaKind::Video, &blob_id, mp4, false)
            .await?;
        self.ack_blob(ChatId(stored.chat), &blob_id).await;
        self.cached_video_path(&blob_id)
            .await?
            .ok_or(ClientError::MediaNotOnDevice)
    }

    /// The path of video message `id`'s clip if this device has it cached; never
    /// touches the network.
    pub async fn cached_video(
        &self,
        id: MessageId,
    ) -> Result<Option<std::path::PathBuf>, ClientError> {
        use crate::messaging::Decoded;
        let Some(stored) = self.shared.store.message(id)? else {
            return Ok(None);
        };
        let Decoded::Video(video) = crate::messaging::decode(&stored.content).body else {
            return Ok(None);
        };
        self.cached_video_path(&video.blob_id).await
    }

    async fn cached_video_path(
        &self,
        blob_id: &crate::media::BlobId,
    ) -> Result<Option<std::path::PathBuf>, ClientError> {
        let store = Arc::clone(&self.shared.store);
        let cache = self.shared.media.clone();
        let (blob_id, now) = (*blob_id, self.now());
        blocking(move || Ok(cache.get_path(&store, &blob_id, now)?)).await
    }

    /// The clip of voice message `id` as the path of an OGG/Opus file in this
    /// device's media cache, for a player to open: from the cache if it is there,
    /// else downloaded, decrypted and cached, and the blob acked so the server can
    /// delete it once everyone has it. The file stays in the core's media
    /// directory; Dart never copies it. Fails with [`ClientError::MediaExpired`]
    /// when the server no longer has it, and with [`ClientError::Offline`] when it
    /// must download and is not connected.
    pub async fn download_audio(&self, id: MessageId) -> Result<std::path::PathBuf, ClientError> {
        use crate::messaging::Decoded;
        let stored = self
            .shared
            .store
            .message(id)?
            .ok_or(ClientError::NoSuchMessage)?;
        let Decoded::Audio(audio) = crate::messaging::decode(&stored.content).body else {
            return Err(ClientError::NoSuchMessage);
        };
        if let Some(path) = self.cached_audio_path(&audio.blob_id).await? {
            return Ok(path);
        }
        if self.shared.store.is_media_expired(&audio.blob_id)? {
            return Err(ClientError::MediaExpired);
        }
        let blob = self.fetch_blob(ChatId(stored.chat), &audio.blob_id).await?;
        let (key, blob_id) = (audio.blob_key, audio.blob_id);
        let ogg = blocking(move || {
            let ogg =
                crate::media::crypto::decrypt(&key, &blob_id, &blob).map_err(MediaError::from)?;
            // Only a clip the core can read is cached and played.
            if !crate::media::audio::is_ogg_opus(&ogg) {
                return Err(MediaError::Audio(crate::media::AudioError::Unsupported).into());
            }
            Ok(ogg)
        })
        .await?;
        self.cache_blob(crate::media::MediaKind::Audio, &blob_id, ogg, false)
            .await?;
        self.ack_blob(ChatId(stored.chat), &blob_id).await;
        self.cached_audio_path(&blob_id)
            .await?
            .ok_or(ClientError::MediaNotOnDevice)
    }

    /// The path of voice message `id`'s clip if this device has it cached; never
    /// touches the network.
    pub async fn cached_audio(
        &self,
        id: MessageId,
    ) -> Result<Option<std::path::PathBuf>, ClientError> {
        use crate::messaging::Decoded;
        let Some(stored) = self.shared.store.message(id)? else {
            return Ok(None);
        };
        let Decoded::Audio(audio) = crate::messaging::decode(&stored.content).body else {
            return Ok(None);
        };
        self.cached_audio_path(&audio.blob_id).await
    }

    async fn cached_audio_path(
        &self,
        blob_id: &crate::media::BlobId,
    ) -> Result<Option<std::path::PathBuf>, ClientError> {
        let store = Arc::clone(&self.shared.store);
        let cache = self.shared.media.clone();
        let (blob_id, now) = (*blob_id, self.now());
        blocking(move || Ok(cache.get_path(&store, &blob_id, now)?)).await
    }

    async fn cache_blob(
        &self,
        kind: crate::media::MediaKind,
        blob_id: &crate::media::BlobId,
        bytes: Vec<u8>,
        own: bool,
    ) -> Result<(), ClientError> {
        let store = Arc::clone(&self.shared.store);
        let cache = self.shared.media.clone();
        let (blob_id, now) = (*blob_id, self.now());
        blocking(move || Ok(cache.put_as(&store, kind, &blob_id, &bytes, own, now)?)).await
    }

    /// Whether chat `chat` takes a message now, as text does: the main group
    /// exists, or the DM exists and is open. (An image waits in the outbox
    /// until its chat can send, like any message.)
    fn check_can_queue(&self, chat: ChatId) -> Result<(), ClientError> {
        if chat.is_main() {
            if self.shared.store.group()?.is_none() {
                return Err(ClientError::NoGroup);
            }
            return Ok(());
        }
        let dm = self
            .shared
            .store
            .dm_by_chat(chat.0)?
            .ok_or(ClientError::NoSuchChat)?;
        if dm.closed_at.is_some() {
            return Err(ClientError::ChatClosed);
        }
        Ok(())
    }

    /// The picture of image message `id`, opened: from this device's media
    /// cache if it is there, else downloaded, decrypted and cached, and the
    /// blob acked so the server can delete it once everyone has it. Returns the
    /// WebP bytes. Fails with [`ClientError::MediaExpired`] when the server no
    /// longer has it, and with [`ClientError::Offline`] when it must download
    /// and is not connected.
    pub async fn download_image(&self, id: MessageId) -> Result<Vec<u8>, ClientError> {
        use crate::messaging::Decoded;
        let stored = self
            .shared
            .store
            .message(id)?
            .ok_or(ClientError::NoSuchMessage)?;
        let Decoded::Image(image) = crate::messaging::decode(&stored.content).body else {
            return Err(ClientError::NoSuchMessage);
        };
        if let Some(webp) = self.cached_webp(&image.blob_id).await? {
            return Ok(webp);
        }
        if self.shared.store.is_media_expired(&image.blob_id)? {
            return Err(ClientError::MediaExpired);
        }
        let blob = self.fetch_blob(ChatId(stored.chat), &image.blob_id).await?;
        let (key, blob_id) = (image.blob_key, image.blob_id);
        let webp = blocking(move || {
            let webp =
                crate::media::crypto::decrypt(&key, &blob_id, &blob).map_err(MediaError::from)?;
            // Only a picture the core can read is cached and shown.
            crate::media::encode::webp_size(&webp)
                .ok_or(MediaError::Image(crate::media::ImageError::Unsupported))?;
            Ok(webp)
        })
        .await?;
        self.cache_webp(&blob_id, webp.clone(), false).await?;
        self.ack_blob(ChatId(stored.chat), &blob_id).await;
        Ok(webp)
    }

    /// Downloads the ciphertext of blob `blob_id` in chat `chat`. A blob the
    /// server no longer has is remembered as expired and fails with
    /// [`ClientError::MediaExpired`] (client-runtime: Expired media).
    async fn fetch_blob(
        &self,
        chat: ChatId,
        blob_id: &crate::media::BlobId,
    ) -> Result<Vec<u8>, ClientError> {
        let group_id = self.group_id_of(chat)?;
        let session = self.session()?;
        let downloaded = session
            .conn()
            .blobs()
            .download_blob(session.request(pb::DownloadBlobRequest {
                group_id,
                blob_id: blob_id.to_vec(),
            })?)
            .await;
        match downloaded {
            Ok(response) => Ok(response.into_inner().ciphertext),
            Err(status) if status.code() == tonic::Code::NotFound => {
                self.shared.store.mark_media_expired(blob_id, self.now())?;
                Err(ClientError::MediaExpired)
            }
            Err(status) => Err(NetError::from(status).into()),
        }
    }

    /// Acks a downloaded blob so the server can delete it once everyone has it.
    /// Best effort: if the ack is lost the blob simply lives its 7 days.
    async fn ack_blob(&self, chat: ChatId, blob_id: &crate::media::BlobId) {
        let acked = async {
            let group_id = self.group_id_of(chat)?;
            let session = self.session()?;
            session
                .conn()
                .blobs()
                .ack_blob(session.request(pb::AckBlobRequest {
                    group_id,
                    blob_id: blob_id.to_vec(),
                })?)
                .await
                .map_err(|status| ClientError::from(NetError::from(status)))
        }
        .await;
        if let Err(error) = acked {
            tracing::warn!(%error, "could not ack a downloaded blob");
        }
    }

    /// The picture of image message `id` if this device has it cached; never
    /// touches the network.
    pub async fn cached_image(&self, id: MessageId) -> Result<Option<Vec<u8>>, ClientError> {
        use crate::messaging::Decoded;
        let Some(stored) = self.shared.store.message(id)? else {
            return Ok(None);
        };
        let Decoded::Image(image) = crate::messaging::decode(&stored.content).body else {
            return Ok(None);
        };
        self.cached_webp(&image.blob_id).await
    }

    async fn cached_webp(
        &self,
        blob_id: &crate::media::BlobId,
    ) -> Result<Option<Vec<u8>>, ClientError> {
        let store = Arc::clone(&self.shared.store);
        let cache = self.shared.media.clone();
        let (blob_id, now) = (*blob_id, self.now());
        blocking(move || Ok(cache.get(&store, &blob_id, now)?)).await
    }

    async fn cache_webp(
        &self,
        blob_id: &crate::media::BlobId,
        webp: Vec<u8>,
        own: bool,
    ) -> Result<(), ClientError> {
        let store = Arc::clone(&self.shared.store);
        let cache = self.shared.media.clone();
        let (blob_id, now) = (*blob_id, self.now());
        blocking(move || Ok(cache.put(&store, &blob_id, &webp, own, now)?)).await
    }

    /// The group id of chat `chat`, whether or not it can send now.
    fn group_id_of(&self, chat: ChatId) -> Result<Vec<u8>, ClientError> {
        if chat.is_main() {
            return Ok(self
                .shared
                .store
                .group()?
                .ok_or(ClientError::NoGroup)?
                .group_id);
        }
        Ok(self
            .shared
            .store
            .dm_by_chat(chat.0)?
            .ok_or(ClientError::NoSuchChat)?
            .group_id)
    }

    /// [`Client::history`] of chat `chat` (change `dms`).
    pub fn history_in(&self, chat: ChatId, limit: usize) -> Result<Vec<Message>, ClientError> {
        let own = self.own_user()?;
        let blocked = self.blocked_set()?;
        Ok(self
            .shared
            .store
            .history_in(chat.0, limit)?
            .iter()
            .filter_map(|stored| crate::messaging::to_message(&self.shared.store, stored, &own))
            .filter(|message| !is_blocked_in(&blocked, message))
            .collect())
    }

    /// This device's block list as a set of `UserId` strings, for filtering a
    /// batch of messages or chats (moderation: A blocked user is hidden).
    fn blocked_set(&self) -> Result<std::collections::HashSet<String>, ClientError> {
        Ok(self.shared.store.blocked_users()?.into_iter().collect())
    }

    /// Marks the mention in message `id` seen; `false` if it had none
    /// unseen (client-runtime: Unseen mentions).
    pub fn mark_mention_seen(&self, id: MessageId) -> Result<bool, ClientError> {
        Ok(self.shared.store.mark_mention_seen(id)?)
    }

    /// Sets this user's display name and "about" text for every member
    /// (message-content: User-owned profiles); `None` clears one. Members
    /// apply it to this user only.
    pub fn set_profile(
        &self,
        display_name: Option<&str>,
        about: Option<&str>,
    ) -> Result<MessageId, ClientError> {
        let profile = ProfileRow {
            display_name: display_name.map(str::to_owned),
            about: about.map(str::to_owned),
        };
        let content = crate::messaging::encode_profile(&profile, self.now()).map_err(too_long)?;
        self.send_content(&content)
    }

    fn send_content(&self, content: &[u8]) -> Result<MessageId, ClientError> {
        self.account()?;
        if self.shared.store.group()?.is_none() {
            return Err(ClientError::NoGroup);
        }
        self.queue_content(crate::store::MAIN_CHAT, content)
    }

    /// Stores an own message in chat `chat` as sending and wakes the engine.
    fn queue_content(&self, chat: i64, content: &[u8]) -> Result<MessageId, ClientError> {
        self.queue_content_with(chat, content, false)
    }

    /// [`Client::queue_content`]; with `blob_pending` the message is marked as
    /// waiting for its blob in the same transaction that stores it, so the
    /// outbox never sees it without the mark.
    fn queue_content_with(
        &self,
        chat: i64,
        content: &[u8],
        blob_pending: bool,
    ) -> Result<MessageId, ClientError> {
        let account = self.account()?;
        let mut msg_id = [0u8; 16];
        getrandom::fill(&mut msg_id)
            .map_err(|e| ClientError::Mls(MlsError::new("randomness", e)))?;
        let user = UserId::new(account.username, account.server_id).to_string();
        let store = &self.shared.store;
        let id = store.transaction(|| -> Result<MessageId, StoreError> {
            let id = store.insert_outgoing_in(
                chat,
                &OutgoingMessage {
                    msg_id,
                    sender_user: &user,
                    sender_device: account.device_id,
                    content,
                    created_at: self.now(),
                },
            )?;
            if blob_pending {
                store.set_blob_pending(id, true)?;
            }
            Ok(id)
        })?;
        if let Some(running) = self.running.lock().as_ref() {
            // A full queue means the engine is busy and flushes anyway.
            let _ = running.commands.try_send(Command::Outbox);
        }
        Ok(id)
    }

    /// Whether any own message is still sending: stored here, not yet accepted
    /// or failed by the server (client-runtime: Unsent messages query). The app
    /// asks it when it goes to the background, to finish sending first.
    pub fn has_unsent(&self) -> Result<bool, ClientError> {
        Ok(!self.shared.store.outgoing_in_flight()?.is_empty())
    }

    /// The newest `limit` chat messages, sent and received, oldest first:
    /// by `server_seq`, then own messages still sending or failed.
    pub fn history(&self, limit: usize) -> Result<Vec<Message>, ClientError> {
        let own = self.own_user()?;
        let blocked = self.blocked_set()?;
        Ok(self
            .shared
            .store
            .history(limit)?
            .iter()
            .filter_map(|stored| crate::messaging::to_message(&self.shared.store, stored, &own))
            .filter(|message| !is_blocked_in(&blocked, message))
            .collect())
    }

    /// One chat message by id.
    pub fn message(&self, id: MessageId) -> Result<Option<Message>, ClientError> {
        let own = self.own_user()?;
        Ok(self
            .shared
            .store
            .message(id)?
            .as_ref()
            .and_then(|stored| crate::messaging::to_message(&self.shared.store, stored, &own)))
    }

    /// This device's user, or one that matches no sender before it is
    /// registered (then there are no messages anyway).
    fn own_user(&self) -> Result<UserId, ClientError> {
        Ok(self.user_id()?.unwrap_or_else(|| UserId::new("", "")))
    }

    /// `user`'s profile, as their devices last set it.
    pub fn profile(&self, user: &UserId) -> Result<Profile, ClientError> {
        let row = self
            .shared
            .store
            .profile(&user.to_string())?
            .unwrap_or_default();
        Ok(Profile {
            display_name: row.display_name,
            about: row.about,
        })
    }

    /// How to show `user` (message-content: User-owned profiles): the display
    /// name with the username alongside, "Ali (alice)", or the username
    /// alone when no display name is set.
    pub fn display_name(&self, user: &UserId) -> Result<String, ClientError> {
        Ok(match self.profile(user)?.display_name {
            Some(name) if !name.is_empty() => format!("{name} ({})", user.username),
            _ => user.username.clone(),
        })
    }

    // --- Group state ---

    /// The group's member devices, as this device sees them.
    pub fn members(&self) -> Result<Vec<Member>, ClientError> {
        let store = &self.shared.store;
        let Some(row) = store.group()? else {
            return Ok(Vec::new());
        };
        let group = store.transaction(|| -> Result<_, ClientError> {
            Ok(crate::mls::group::load(store.mls(), &row.group_id)?)
        })?;
        let Some(group) = group else {
            return Ok(Vec::new());
        };
        let mut members: Vec<Member> = verify::members(&group)
            .into_iter()
            .map(|(_, claim)| Member {
                user: claim.claimed.user,
                device: claim.claimed.device_id,
            })
            .collect();
        members.sort();
        Ok(members)
    }

    /// The admin's user, as the lobby group's context names it (the
    /// `0xF0A1` extension every member holds); `None` before this device is in
    /// the group. Lets a non-admin's app mark the admin in the member list.
    pub fn admin_user(&self) -> Result<Option<UserId>, ClientError> {
        let store = &self.shared.store;
        let Some(row) = store.group()? else {
            return Ok(None);
        };
        let group = store.transaction(|| -> Result<_, ClientError> {
            Ok(crate::mls::group::load(store.mls(), &row.group_id)?)
        })?;
        Ok(group
            .and_then(|g| crate::mls::group::admin_user_id(g.extensions()))
            .and_then(|text| UserId::parse(&text)))
    }

    /// The group's epoch on this device, once it is in the group.
    pub fn epoch(&self) -> Result<Option<u64>, ClientError> {
        let store = &self.shared.store;
        let Some(row) = store.group()? else {
            return Ok(None);
        };
        let group = store.transaction(|| -> Result<_, ClientError> {
            Ok(crate::mls::group::load(store.mls(), &row.group_id)?)
        })?;
        Ok(group.map(|g| g.epoch().as_u64()))
    }
}

/// Whether `message` should be hidden because its sender is blocked
/// (moderation: A blocked user is hidden). Own messages are never hidden.
fn is_blocked_in(blocked: &std::collections::HashSet<String>, message: &Message) -> bool {
    !message.own_user && blocked.contains(&message.sender.to_string())
}

/// The text a report quotes for a stored message (moderation: Report a DM
/// message): the text of a text message or an image's or video's caption, else
/// empty. The caller caps it with the reply-quote rules.
fn quotable_text(content: &[u8]) -> String {
    match crate::messaging::decode(content).body {
        crate::messaging::Decoded::Text(text) => text.text,
        crate::messaging::Decoded::Image(image) => image.caption.unwrap_or_default(),
        crate::messaging::Decoded::Video(video) => video.caption.unwrap_or_default(),
        _ => String::new(),
    }
}

fn too_long(error: ContentError) -> ClientError {
    match error {
        ContentError::TooLong => ClientError::TooLong,
        ContentError::InvalidMention => ClientError::InvalidMention,
    }
}

fn invite_info(invite: &pb::Invite) -> Result<InviteInfo, ClientError> {
    Ok(InviteInfo {
        code: invite.code.clone(),
        user: invite
            .user_id
            .as_ref()
            .map(UserId::from_pb)
            .ok_or_else(|| NetError::Protocol("an invite without user_id".into()))?,
        expires_at: invite
            .expires_at
            .as_ref()
            .map(from_timestamp)
            .ok_or_else(|| NetError::Protocol("an invite without expires_at".into()))?,
        payload: Vec::new(),
    })
}
