//! The public [`Client`] (client-core design D2).
//!
//! A device's life: [`Client::open`]; then once either [`Client::register`]
//! (first device, from an invite) or [`Client::link_request`] +
//! [`Client::complete_link`] (another device of a user); then
//! [`Client::start`], which connects and keeps the device in the group. Admin
//! actions ([`Client::create_group`], [`Client::add_user`],
//! [`Client::remove_user`], invites) and linking other devices need the
//! client to be started and connected. Messaging (`send_text`, ...) is task
//! group 6.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use parking_lot::Mutex;
use protocol::wire_transport::PROTOCOL_VERSION;
use tokio::sync::{mpsc, oneshot};

use crate::clock::{Clock, SystemClock};
use crate::engine::{Command, Engine, Job, Shared};
use crate::events::{
    self, DeviceId, Event, EventStream, InviteInfo, Member, PushPlatform, PushToken, UserId,
};
use crate::identity::certificate::{self, from_timestamp};
use crate::identity::keys::{create_identity_key, identity_key};
use crate::identity::{DeviceKeys, Invite, LinkApproval, LinkRequest, PayloadError};
use crate::keystore::{KeyStore, KeyStoreError, KeyStoreKind};
use crate::mls::MlsError;
use crate::mls::verify;
use crate::net::{Conn, NetError, Session};
use crate::pb;
use crate::store::{AccountRow, Store, StoreError};

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
}

impl ClientConfig {
    /// A config on the system clock.
    pub fn new(data_dir: impl Into<PathBuf>, key_store: Arc<dyn KeyStore>) -> Self {
        Self {
            data_dir: data_dir.into(),
            key_store,
            clock: Arc::new(SystemClock),
        }
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
    /// Talking to the Pi failed; [`NetError::ServerIdentity`] means the pin
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
    /// The Pi behind the invite is not the one it names.
    #[error("the server does not match the invite: {0}")]
    ServerMismatch(String),
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
    /// The Pi knows no such user.
    #[error("unknown user {0}")]
    UnknownUser(UserId),
    /// The user has no KeyPackages on the Pi yet, so they cannot be added.
    #[error("{0} has no KeyPackages yet")]
    NoKeyPackages(UserId),
    /// The commit would be larger than the handshake limit.
    #[error("the commit is larger than the server accepts")]
    TooLarge,
    /// The Pi rejected the commit.
    #[error("the server rejected the commit: {0}")]
    Rejected(String),
    /// Not possible yet; retried later.
    #[error("not possible yet")]
    NotYet,
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
    if !info.supported_protocol_versions.contains(&PROTOCOL_VERSION) {
        return Err(ClientError::ServerMismatch(format!(
            "it does not speak protocol version {PROTOCOL_VERSION}"
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
        } = config;
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

    /// The local store, for the feature modules.
    #[allow(dead_code)] // used by task group 6
    pub(crate) fn store(&self) -> &Store {
        &self.shared.store
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
            let keys = DeviceKeys::load_or_create(store.as_ref())?;
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
        let registered = conn
            .devices()
            .register(pb::RegisterRequest {
                invite_code: invite.code.clone(),
                identity_public_key: identity_public.clone(),
                certificate: Some(signed.clone()),
            })
            .await
            .map_err(NetError::from)?
            .into_inner();
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
        let keys = blocking(move || Ok(DeviceKeys::load_or_create(store.as_ref())?)).await?;
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
        // The user's identity key, from the Pi (pinned on first sight), with
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
        let keys =
            blocking(move || DeviceKeys::load(store.as_ref())?.ok_or(ClientError::NotRegistered))
                .await?;
        self.stop();
        let (commands, receiver) = mpsc::channel(64);
        let engine = Engine::new(Arc::clone(&self.shared), keys, account, receiver)
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

    /// Whether the client has an authenticated connection right now.
    pub fn is_connected(&self) -> bool {
        self.shared.session.lock().is_some()
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

    /// Registers, replaces or (with `None`) clears this device's push token
    /// (device-identity: Push token registration). Real push arrives with the
    /// Flutter app; this is the placeholder the core already supports.
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
    /// their devices works again. Returns `false` if no new key is pending.
    pub fn accept_identity_key(&self, user: &UserId) -> Result<bool, ClientError> {
        let id = user.to_string();
        let Some(pinned) = self.shared.store.pinned_identity_key(&id)? else {
            return Ok(false);
        };
        let Some(pending) = pinned.pending else {
            return Ok(false);
        };
        Ok(self
            .shared
            .store
            .accept_identity_key(&id, &pending, self.now())?)
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
