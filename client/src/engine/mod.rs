//! The engine: one task that owns the connection to the server, the `Connect`
//! stream and the MLS group, and does everything that touches them in order
//! (client-core design D4, D5, D6).
//!
//! ```text
//! loop {
//!     connect if needed (pinned TLS, challenge auth, Connect + Hello; backoff)
//!     select (frames first) {
//!         ServerFrame   -> envelopes in server_seq order, send results, notices
//!         Command       -> queue a job (admin actions, revoke, ...)
//!         tick          -> heartbeat, liveness, token renewal, rotation and
//!                          last-resort checks, linked devices
//!         quiet stream  -> run the next job (one own commit at a time)
//!     }
//! }
//! ```
//!
//! **Own commits.** One at a time (OpenMLS keeps one pending commit). The
//! envelope is stored before it is sent and resent identically after a
//! reconnect or crash (message-delivery: Idempotent sends). While it waits
//! for its result, every incoming envelope waits too: a commit from someone
//! else means ours will be `STALE_EPOCH`, and what comes after our commit is
//! in the next epoch. The server delivers every entry below the commit's
//! `server_seq` before `Accepted` (message-delivery: Ordered delivery and
//! catch-up), so on `Accepted` the held envelopes below it are read in the
//! old epoch, then the commit is merged, then the rest follow. On
//! `STALE_EPOCH` the job goes back to the front of the queue and runs again
//! once the device has caught up to the epoch the server named.
//!
//! **Incoming.** Strictly in `server_seq` order; each is processed and
//! recorded (with `processed_seq`) in one store transaction and acked after
//! it commits (client-runtime: Local message store). Commits and Welcomes are
//! checked with [`crate::mls::verify`] first.

mod jobs;
mod outbox;
mod uploads;

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use openmls::prelude::tls_codec::Deserialize as _;
use openmls::prelude::{
    ExternalSender, MlsGroup, MlsMessageBodyIn, MlsMessageIn, ProcessedMessageContent,
    ProtocolMessage, Sender, SenderExtensionIndex, StagedWelcome,
};
use openmls_traits::OpenMlsProvider as _;
use parking_lot::Mutex;
use prost::Message as _;
use protocol::wire_transport::{
    HEARTBEAT_INTERVAL, STREAM_IDLE_TIMEOUT, SUPPORTED_PROTOCOL_VERSIONS,
    negotiate_protocol_version,
};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;

use crate::client::ClientError;
use crate::clock::Clock;
use crate::events::{ConnectionState, DeviceId, Event, EventSender, Member, MessageId, UserId};
use crate::identity::DeviceKeys;
use crate::keystore::{KeyName, KeyStore};
use crate::mls::verify::{self, LeafProblem};
use crate::mls::{MlsError, group as mls_group};
use crate::net::{Backoff, Conn, NetError, Session};
use crate::pb;
use crate::pb::client_frame::Frame as Out;
use crate::pb::envelope::Kind;
use crate::pb::server_frame::Frame as In;
use crate::store::{AccountRow, OutgoingCommitRow, PinOutcome, Store, StoreError};

pub(crate) use jobs::Job;

/// How often the engine looks at its timers. Deadlines are read from the
/// injectable clock (design D7), so tests move time without waiting.
pub(crate) const TICK: Duration = Duration::from_millis(100);

/// How long a queued image waits before its upload is tried again after a
/// failure that may pass (media-blobs: Blob crypto and send order).
pub(crate) const UPLOAD_RETRY_DELAY: Duration = Duration::from_secs(3);

/// Jobs run only once the stream has been quiet this long, so a burst of
/// catch-up is processed first.
pub(crate) const JOB_QUIET: Duration = Duration::from_millis(100);

/// How often a device retries adding a device it linked that has not
/// uploaded KeyPackages yet.
pub(crate) const PENDING_LINK_POLL: Duration = Duration::from_secs(1);

/// The longest a TCP + TLS + HTTP/2 connect may take.
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Commands the [`crate::Client`] sends the engine. (Stopping aborts the
/// task.)
pub(crate) enum Command {
    /// Queue a job.
    Job(Job),
    /// An own message was stored: send it when possible.
    Outbox,
    /// The upload of a queued image's blob ended.
    Uploaded {
        id: MessageId,
        outcome: uploads::Outcome,
    },
    /// The network is back: skip the current backoff wait if disconnected
    /// (client-runtime: Connection lifecycle, `reconnect_now`).
    ReconnectNow,
}

/// What every part of the client shares.
pub(crate) struct Shared {
    pub(crate) store: Arc<Store>,
    pub(crate) key_store: Arc<dyn KeyStore>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) events: EventSender,
    /// The engine's current session, for the client's unary calls.
    pub(crate) session: Mutex<Option<Session>>,
    /// Whether the server last connected to lists protocol version 2, so knows
    /// DMs (wire-transport: Protocol version negotiation, change `dms`).
    pub(crate) server_knows_dms: std::sync::atomic::AtomicBool,
    /// Whether it lists protocol version 3, so knows named groups (change
    /// `lobby-multi-group`).
    pub(crate) server_knows_named_groups: std::sync::atomic::AtomicBool,
    /// The decrypted images this device keeps (client-runtime: Image download
    /// and cache).
    pub(crate) media: crate::media::MediaCache,
}

/// A reply to a job.
pub(crate) type Reply = oneshot::Sender<Result<(), ClientError>>;

/// One open `Connect` stream on an authenticated connection.
struct Link {
    session: Session,
    tx: mpsc::Sender<pb::ClientFrame>,
    rx: tonic::Streaming<pb::ServerFrame>,
    last_sent: Instant,
    last_received: Instant,
}

/// The own commit in flight.
struct InFlight {
    row: OutgoingCommitRow,
    job: Job,
    /// The group the commit is for (change `dms`).
    group_id: Vec<u8>,
    /// Whether it went out on the current link.
    sent: bool,
    /// `Accepted` arrived with this `server_seq`; merged once the held
    /// envelopes below it are read.
    accepted: Option<u64>,
}

/// Why the engine loop stopped processing a frame or job.
#[derive(Debug, thiserror::Error)]
pub(crate) enum EngineError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Mls(#[from] MlsError),
    #[error(transparent)]
    Net(#[from] NetError),
    /// The link broke while sending.
    #[error("the connection broke")]
    LinkLost,
}

impl From<EngineError> for ClientError {
    fn from(error: EngineError) -> Self {
        match error {
            EngineError::Store(e) => ClientError::Store(e),
            EngineError::Mls(e) => ClientError::Mls(e),
            EngineError::Net(e) => ClientError::Net(e),
            EngineError::LinkLost => ClientError::Offline,
        }
    }
}

pub(crate) struct Engine {
    shared: Arc<Shared>,
    keys: DeviceKeys,
    account: AccountRow,
    server: ExternalSender,
    commands: mpsc::Receiver<Command>,
    /// Lets a task the engine started (an image upload) report back.
    command_tx: mpsc::Sender<Command>,
    /// The blob uploads of queued image messages, by message.
    uploads: BTreeMap<MessageId, uploads::Upload>,
    /// The main group.
    group: Option<MlsGroup>,
    /// The DMs this device has MLS state for, by group id (change `dms`).
    dms: BTreeMap<Vec<u8>, MlsGroup>,
    link: Option<Link>,
    /// The session token to resume with on the next reconnect, kept across a
    /// dropped link (wire-transport: Session resume; client-runtime:
    /// Connection lifecycle). Set when a token is obtained or renewed, cleared
    /// only when the device is removed.
    resume: Option<crate::net::ResumeToken>,
    /// The token last written to the key store, so an unchanged one is not
    /// written again (client-runtime: Resume across restart).
    saved_resume: Option<crate::net::ResumeToken>,
    backoff: Backoff,
    next_attempt: Instant,
    /// A pin mismatch or a revoked device stops connecting until the client
    /// starts again.
    halted: bool,
    jobs: VecDeque<Job>,
    inflight: Option<InFlight>,
    /// Incoming envelopes held while an own commit waits for its result.
    deferred: VecDeque<pb::Envelope>,
    /// The held envelopes are being processed (so one taken from the front
    /// is not held again behind the rest).
    draining: bool,
    /// Own application messages sent on the current link and waiting for
    /// their result, by msg_id.
    apps_in_flight: BTreeMap<[u8; 16], MessageId>,
    /// The epoch the server said each group is at (from `STALE_EPOCH`).
    server_epochs: BTreeMap<Vec<u8>, u64>,
    quiet_since: Instant,
    last_rotation_check: Option<SystemTime>,
    rotation_at: Option<SystemTime>,
    last_link_poll: Instant,
}

fn random_msg_id() -> Result<[u8; 16], MlsError> {
    let mut id = [0u8; 16];
    getrandom::fill(&mut id).map_err(|e| MlsError::new("randomness", e))?;
    Ok(id)
}

/// `<username>@<server_id>` as a [`UserId`].
fn parse_user_id(text: &str) -> Option<UserId> {
    let (username, server_id) = text.split_once('@')?;
    Some(UserId::new(username, server_id))
}

/// qa-fixes D5, inside the merge's transaction: devices the server's inactivity
/// Removes took out are recorded; an added device recorded so (or removed and
/// added in the same commit) is a re-add. Returns both kinds, for the UI to
/// stay quiet about.
fn automatic_changes(
    store: &Store,
    review: &verify::CommitReview,
    now: SystemTime,
) -> Result<Vec<Member>, StoreError> {
    let mut automatic = Vec::new();
    for (user, device) in &review.removed {
        if review.server_removed.contains(device) {
            store.mark_inactive(device, now)?;
            automatic.push(Member {
                user: user.clone(),
                device: *device,
            });
        }
    }
    for (user, device) in &review.added {
        let was_inactive = store.take_inactive(device)?;
        let replaced = review.removed.iter().any(|(_, removed)| removed == device);
        if was_inactive || replaced {
            automatic.push(Member {
                user: user.clone(),
                device: *device,
            });
        }
    }
    Ok(automatic)
}

/// The [`Store::flag`] set once this core uploaded KeyPackages that list the
/// DM and named-group extensions (version 3; a version 2 core's "dm_key_packages"
/// flag does not count, so it uploads a fresh set on its first version 3 run).
pub(crate) const DM_KEY_PACKAGES_FLAG: &str = "named_key_packages";

/// The group with id `group_id` among the main group and the DMs, mutably;
/// borrows only those two fields, so other engine fields stay usable.
fn group_in<'a>(
    main: &'a mut Option<MlsGroup>,
    dms: &'a mut BTreeMap<Vec<u8>, MlsGroup>,
    group_id: &[u8],
) -> Option<&'a mut MlsGroup> {
    match main {
        Some(group) if group.group_id().as_slice() == group_id => Some(group),
        _ => dms.get_mut(group_id),
    }
}

fn pending_forever<T>() -> std::future::Pending<T> {
    std::future::pending()
}

impl Engine {
    /// Prepares the engine for a registered device: loads the group and any
    /// own commit left in flight by a crash.
    pub(crate) fn new(
        shared: Arc<Shared>,
        keys: DeviceKeys,
        account: AccountRow,
        commands: mpsc::Receiver<Command>,
        command_tx: mpsc::Sender<Command>,
        resume: Option<crate::net::ResumeToken>,
    ) -> Result<Self, EngineError> {
        let server = mls_group::server_external_sender(
            &account.pi_sender_key,
            &account.pi_sender_credential,
        )?;
        let group = match shared.store.group()? {
            Some(row) => mls_group::load(shared.store.mls(), &row.group_id)?,
            None => None,
        };
        let mut dms = BTreeMap::new();
        for dm in shared.store.dms()? {
            if dm.joined
                && dm.closed_at.is_none()
                && let Some(group) = mls_group::load(shared.store.mls(), &dm.group_id)?
            {
                dms.insert(dm.group_id, group);
            }
        }
        let mut engine = Self {
            shared,
            keys,
            account,
            server,
            commands,
            command_tx,
            uploads: BTreeMap::new(),
            group,
            dms,
            link: None,
            saved_resume: resume.clone(),
            resume,
            backoff: Backoff::new(),
            next_attempt: Instant::now(),
            halted: false,
            jobs: VecDeque::new(),
            inflight: None,
            deferred: VecDeque::new(),
            draining: false,
            apps_in_flight: BTreeMap::new(),
            server_epochs: BTreeMap::new(),
            quiet_since: Instant::now(),
            last_rotation_check: None,
            rotation_at: None,
            last_link_poll: Instant::now(),
        };
        if let Some(row) = engine.shared.store.outgoing_commit()? {
            let group_id = pb::Envelope::decode(row.envelope.as_slice())
                .map(|e| e.group_id)
                .unwrap_or_default();
            let pending = engine
                .group_ref(&group_id)
                .is_some_and(|g| g.pending_commit().is_some());
            if pending {
                tracing::info!("resending the own commit left in flight");
                let job = Job::recovered(&row.purpose);
                engine.inflight = Some(InFlight {
                    row,
                    job,
                    group_id,
                    sent: false,
                    accepted: None,
                });
            } else {
                engine.shared.store.clear_outgoing_commit()?;
            }
        }
        // direct-messages: Old apps never join a DM. A version 2 core uploads
        // a fresh set of KeyPackages that list 0xF0A2 the first time it runs.
        if engine.shared.store.last_resort_uploaded_at()?.is_none()
            || !engine.shared.store.flag(DM_KEY_PACKAGES_FLAG)?
        {
            engine.push_job(Job::UploadKeyPackages { remaining: Some(0) });
        }
        // group-membership: Re-add after an unusable Welcome. A device with
        // no group asks once per start; the server tells a stuck one from one
        // still waiting for its first add. Not the admin's: before the group
        // exists the admin's first device creates it, and a linked admin
        // device asks when it refuses its Welcome.
        if !engine.account.is_admin && engine.shared.store.group()?.is_none() {
            engine.push_job(Job::RequestReadd {
                refused_seq: 0,
                reason: "no group at start".into(),
            });
        }
        Ok(engine)
    }

    fn now(&self) -> SystemTime {
        self.shared.clock.now()
    }

    fn store(&self) -> &Store {
        &self.shared.store
    }

    // --- Groups (change `dms`: the main group and the DMs) ---

    /// The group with id `group_id`: the lobby, a named group or a DM.
    fn group_ref(&self, group_id: &[u8]) -> Option<&MlsGroup> {
        match &self.group {
            Some(group) if group.group_id().as_slice() == group_id => Some(group),
            _ => self.dms.get(group_id),
        }
    }

    /// The group with id `group_id`, mutably.
    fn group_mut(&mut self, group_id: &[u8]) -> Option<&mut MlsGroup> {
        group_in(&mut self.group, &mut self.dms, group_id)
    }

    /// Takes the group with id `group_id` out, to work on it while calling
    /// other engine methods; put it back with [`Engine::put_group`].
    fn take_group(&mut self, group_id: &[u8]) -> Option<MlsGroup> {
        if self
            .group
            .as_ref()
            .is_some_and(|g| g.group_id().as_slice() == group_id)
        {
            return self.group.take();
        }
        self.dms.remove(group_id)
    }

    /// Puts a group back where it belongs: a DM or named group by its context,
    /// else the lobby.
    fn put_group(&mut self, group: MlsGroup) {
        if mls_group::kind_of(group.extensions()) != protocol::group_membership::GroupKind::System {
            self.dms.insert(group.group_id().as_slice().to_vec(), group);
        } else {
            self.group = Some(group);
        }
    }

    /// The main group's id, once it has one.
    fn main_id(&self) -> Option<Vec<u8>> {
        self.group
            .as_ref()
            .map(|g| g.group_id().as_slice().to_vec())
    }

    /// Reloads `group_id` from the store after a failed transaction.
    fn reload_group_in(&mut self, group_id: &[u8]) -> Result<(), EngineError> {
        if self.store().is_main(group_id)? || self.main_id().as_deref() == Some(group_id) {
            return self.reload_group();
        }
        match mls_group::load(self.store().mls(), group_id)? {
            Some(group) => {
                self.dms.insert(group_id.to_vec(), group);
            }
            None => {
                self.dms.remove(group_id);
            }
        }
        Ok(())
    }

    fn emit(&self, event: Event) {
        self.shared.events.emit(event);
    }

    fn user(&self) -> UserId {
        UserId::new(
            self.account.username.clone(),
            self.account.server_id.clone(),
        )
    }

    fn device_ref(&self) -> pb::DeviceRef {
        crate::membership::device_ref(&self.user(), &self.account.device_id)
    }

    fn session(&self) -> Result<Session, EngineError> {
        self.link
            .as_ref()
            .map(|l| l.session.clone())
            .ok_or(EngineError::LinkLost)
    }

    /// Queues `job` unless an equal one is queued already.
    pub(crate) fn push_job(&mut self, job: Job) {
        if job.is_idempotent() && self.jobs.iter().any(|queued| queued.same_as(&job)) {
            return;
        }
        self.jobs.push_back(job);
    }

    /// Runs until stopped.
    pub(crate) async fn run(mut self) {
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            if self.link.is_none() && !self.halted && Instant::now() >= self.next_attempt {
                self.connect().await;
            }
            let can_run_job = self.can_run_job();
            let job_at = tokio::time::Instant::from_std(self.quiet_since + JOB_QUIET);
            let reconnect_at = tokio::time::Instant::from_std(self.next_attempt);
            let waiting_to_connect = self.link.is_none() && !self.halted;
            tokio::select! {
                biased;
                frame = async {
                    match self.link.as_mut() {
                        Some(link) => link.rx.message().await,
                        None => pending_forever().await,
                    }
                } => match frame {
                    Ok(Some(frame)) => {
                        if let Some(link) = self.link.as_mut() {
                            link.last_received = Instant::now();
                        }
                        self.quiet_since = Instant::now();
                        if let Err(error) = self.on_frame(frame).await {
                            self.on_error(error);
                        }
                        if let Err(error) = self.flush_outbox().await {
                            self.on_error(error);
                        }
                    }
                    Ok(None) => self.drop_link("the server closed the stream"),
                    Err(status) if status.code() == tonic::Code::PermissionDenied => {
                        self.removed();
                    }
                    Err(status) => {
                        let why = format!("stream ended: {status}");
                        self.drop_link(&why);
                    }
                },
                command = self.commands.recv() => match command {
                    Some(Command::Job(job)) => self.push_job(job),
                    Some(Command::Outbox) => {
                        if let Err(error) = self.flush_outbox().await {
                            self.on_error(error);
                        }
                    }
                    Some(Command::Uploaded { id, outcome }) => {
                        if let Err(error) = self.on_uploaded(id, outcome).await {
                            self.on_error(error);
                        }
                    }
                    Some(Command::ReconnectNow) => self.reconnect_now(),
                    None => break,
                },
                _ = tick.tick() => {
                    if let Err(error) = self.on_tick().await {
                        self.on_error(error);
                    }
                }
                () = tokio::time::sleep_until(job_at), if can_run_job => {
                    if let Err(error) = self.run_next_job().await {
                        self.on_error(error);
                    }
                }
                () = tokio::time::sleep_until(reconnect_at), if waiting_to_connect => {}
            }
        }
        self.link = None;
        *self.shared.session.lock() = None;
        self.emit(Event::Connection(ConnectionState::Disconnected));
    }

    fn on_error(&mut self, error: EngineError) {
        match error {
            EngineError::LinkLost => self.drop_link("the connection broke"),
            EngineError::Net(NetError::Connect(why)) => self.drop_link(&why),
            other => tracing::error!(error = %other, "engine step failed"),
        }
    }

    // --- Connection lifecycle (client-runtime: Connection lifecycle) ---

    async fn connect(&mut self) {
        self.emit(Event::Connection(ConnectionState::Connecting));
        let attempt = tokio::time::timeout(CONNECT_TIMEOUT, self.open_link()).await;
        match attempt {
            Ok(Ok(link)) => {
                self.backoff.reset();
                *self.shared.session.lock() = Some(link.session.clone());
                // Keep the token to resume the next reconnect with (wire-transport:
                // Session resume).
                self.resume = Some(link.session.to_resume());
                self.link = Some(link);
                self.quiet_since = Instant::now();
                self.last_rotation_check = None;
                tracing::info!("connected");
                self.emit(Event::Connection(ConnectionState::Connected));
                self.save_resume().await;
                self.retry_uploads_now();
                if let Err(error) = self.resend_inflight().await {
                    self.on_error(error);
                }
                if let Err(error) = self.flush_outbox().await {
                    self.on_error(error);
                }
            }
            Ok(Err(NetError::ServerIdentity)) => {
                self.halted = true;
                self.emit(Event::ServerIdentityError);
                self.emit(Event::Connection(ConnectionState::Disconnected));
            }
            Ok(Err(NetError::Rpc(status))) if status.code() == tonic::Code::PermissionDenied => {
                self.removed();
            }
            Ok(Err(error)) => {
                tracing::debug!(%error, "connect failed");
                self.next_attempt = Instant::now() + self.backoff.next_delay();
            }
            Err(_) => {
                tracing::debug!("connect timed out");
                self.next_attempt = Instant::now() + self.backoff.next_delay();
            }
        }
    }

    /// The server said this device is revoked (`PERMISSION_DENIED` from
    /// `Authenticate` or the stream): emit [`Event::Removed`] once and stop
    /// connecting until the client starts again. Local history stays.
    fn removed(&mut self) {
        if self.halted {
            return;
        }
        tracing::info!("this device was revoked; not reconnecting");
        self.halted = true;
        self.link = None;
        *self.shared.session.lock() = None;
        self.resume = None;
        self.forget_saved_resume();
        self.deferred.clear();
        self.apps_in_flight.clear();
        if let Some(inflight) = self.inflight.as_mut() {
            inflight.sent = false;
        }
        self.emit(Event::Removed);
        self.emit(Event::Connection(ConnectionState::Disconnected));
    }

    /// Writes the current resume token to the key store unless it is already
    /// there (client-runtime: Resume across restart). The key store can block,
    /// so it runs off the engine task; a failure only costs the next start the
    /// full handshake.
    async fn save_resume(&mut self) {
        let Some(resume) = self.resume.clone() else {
            return;
        };
        if self
            .saved_resume
            .as_ref()
            .is_some_and(|saved| saved.same_token(&resume))
        {
            return;
        }
        let store = Arc::clone(&self.shared.key_store);
        let value = resume.encode();
        let written =
            tokio::task::spawn_blocking(move || store.set(KeyName::SessionToken, &value)).await;
        match written {
            Ok(Ok(())) => self.saved_resume = Some(resume),
            Ok(Err(error)) => tracing::warn!(%error, "could not save the resume token"),
            Err(error) => tracing::warn!(%error, "could not save the resume token"),
        }
    }

    /// Deletes the saved resume token from the key store, in the background:
    /// nothing connects after this.
    fn forget_saved_resume(&mut self) {
        self.saved_resume = None;
        let store = Arc::clone(&self.shared.key_store);
        tokio::task::spawn_blocking(move || {
            if let Err(error) = store.delete(KeyName::SessionToken) {
                tracing::warn!(%error, "could not delete the resume token");
            }
        });
    }

    /// Skips the current backoff wait if disconnected; nothing if connected,
    /// connecting or halted.
    fn reconnect_now(&mut self) {
        if self.link.is_none() && !self.halted {
            tracing::debug!("reconnecting now");
            self.next_attempt = Instant::now();
        }
    }

    /// Pinned TLS, then the `Connect` stream. On reconnect this reuses a
    /// still-valid session token to open the stream directly, skipping
    /// `GetChallenge`/`Authenticate`; otherwise, and when the server rejects the
    /// token, it runs the full challenge/auth handshake (wire-transport:
    /// Session resume; client-runtime: Connection lifecycle).
    async fn open_link(&self) -> Result<Link, NetError> {
        let conn = Conn::open(&self.account.relay_address, self.account.tls_pin).await?;
        // wire-transport: Protocol version negotiation. Version 2 with a server
        // that knows DMs, version 1 with an older one.
        let info = conn.server_info().await?;
        let protocol_version = negotiate_protocol_version(&info.supported_protocol_versions)
            .ok_or_else(|| {
                NetError::Protocol(format!(
                    "the server speaks none of the protocol versions {SUPPORTED_PROTOCOL_VERSIONS:?}"
                ))
            })?;
        self.shared.server_knows_named_groups.store(
            protocol::wire_transport::server_knows_named_groups(&info.supported_protocol_versions),
            std::sync::atomic::Ordering::Relaxed,
        );
        self.shared.server_knows_dms.store(
            protocol::wire_transport::server_knows_dms(&info.supported_protocol_versions),
            std::sync::atomic::Ordering::Relaxed,
        );
        // wire-transport: Session resume. A token kept from the last connection
        // and not yet due for renewal opens the stream with no round trips for
        // the challenge and `Authenticate`. The server rejecting it on the stream
        // (`UNAUTHENTICATED`: evicted, clock skew, or revoked) falls through to
        // the full handshake, which is also where a revoked device meets
        // `PERMISSION_DENIED` (design D2).
        if let Some(resume) = self.resume.as_ref().filter(|r| r.usable(self.now())) {
            match Self::open_stream(conn.resume(resume), protocol_version).await {
                Ok(link) => return Ok(link),
                Err(NetError::Rpc(status)) if status.code() == tonic::Code::Unauthenticated => {
                    tracing::debug!("the resume token was rejected; full handshake");
                }
                Err(other) => return Err(other),
            }
        }
        let session = conn
            .authenticate(&self.keys, self.device_ref(), self.now())
            .await?;
        Self::open_stream(session, protocol_version).await
    }

    /// Opens the `Connect` stream on `session`'s connection, with `Hello` as
    /// the first frame.
    async fn open_stream(session: Session, protocol_version: u32) -> Result<Link, NetError> {
        let (tx, frames) = mpsc::channel(64);
        tx.send(pb::ClientFrame {
            frame: Some(Out::Hello(pb::Hello { protocol_version })),
        })
        .await
        .map_err(|_| NetError::Connect("stream closed".into()))?;
        let request = session.request(ReceiverStream::new(frames))?;
        let rx = session
            .conn()
            .messaging()
            .connect(request)
            .await?
            .into_inner();
        Ok(Link {
            session,
            tx,
            rx,
            last_sent: Instant::now(),
            last_received: Instant::now(),
        })
    }

    fn drop_link(&mut self, why: &str) {
        if self.link.take().is_none() {
            return;
        }
        tracing::info!(why, "disconnected; reconnecting");
        *self.shared.session.lock() = None;
        // Held envelopes were not acked (acks are cumulative and stop before
        // the first held one), so the server delivers them again; own messages
        // in flight are sent again with the same bytes.
        self.deferred.clear();
        self.apps_in_flight.clear();
        if let Some(inflight) = self.inflight.as_mut() {
            inflight.sent = false;
        }
        self.next_attempt = Instant::now() + self.backoff.next_delay();
        self.emit(Event::Connection(ConnectionState::Connecting));
    }

    async fn send(&mut self, frame: Out) -> Result<(), EngineError> {
        let link = self.link.as_mut().ok_or(EngineError::LinkLost)?;
        link.tx
            .send(pb::ClientFrame { frame: Some(frame) })
            .await
            .map_err(|_| EngineError::LinkLost)?;
        link.last_sent = Instant::now();
        Ok(())
    }

    async fn ack(&mut self, group_id: &[u8], server_seq: u64) -> Result<(), EngineError> {
        self.send(Out::Ack(pb::Ack {
            group_id: group_id.to_vec(),
            server_seq,
        }))
        .await
    }

    /// Heartbeats, liveness, token renewal and the timed checks.
    async fn on_tick(&mut self) -> Result<(), EngineError> {
        // Held envelopes left by a step that failed.
        if !self.deferred.is_empty() && self.inflight.as_ref().is_none_or(|f| f.accepted.is_some())
        {
            self.process_deferred().await?;
        }
        // A queued image whose upload waited to retry.
        if self.link.is_some() && self.upload_due() {
            self.flush_outbox().await?;
        }
        let Some(link) = self.link.as_ref() else {
            return Ok(());
        };
        // wire-transport: Heartbeats.
        if link.last_sent.elapsed() >= HEARTBEAT_INTERVAL {
            self.send(Out::Heartbeat(pb::Heartbeat {})).await?;
        }
        let Some(link) = self.link.as_ref() else {
            return Ok(());
        };
        if link.last_received.elapsed() > STREAM_IDLE_TIMEOUT {
            self.drop_link("the server went quiet");
            return Ok(());
        }
        // Renew the session token before it expires, on the same connection.
        if link.session.needs_renewal(self.now()) {
            let conn = link.session.conn().clone();
            match conn
                .authenticate(&self.keys, self.device_ref(), self.now())
                .await
            {
                Ok(session) => {
                    *self.shared.session.lock() = Some(session.clone());
                    // Resume the next reconnect with the renewed token.
                    self.resume = Some(session.to_resume());
                    self.save_resume().await;
                    if let Some(link) = self.link.as_mut() {
                        link.session = session;
                    }
                    tracing::debug!("renewed the session token");
                }
                Err(NetError::Rpc(status)) if status.code() == tonic::Code::PermissionDenied => {
                    self.removed();
                    return Ok(());
                }
                Err(error) => {
                    tracing::warn!(%error, "token renewal failed");
                    self.drop_link("token renewal failed");
                    return Ok(());
                }
            }
        }
        self.timed_checks()?;
        self.flush_outbox().await
    }

    /// Own-leaf rotation, last-resort refresh (decision #4, design D6) and
    /// linked devices waiting to be added.
    fn timed_checks(&mut self) -> Result<(), EngineError> {
        let now = self.now();
        let due_check = self.last_rotation_check.is_none_or(|last| {
            now.duration_since(last).unwrap_or_default()
                >= protocol::group_membership::ROTATION_CHECK_INTERVAL
        });
        if due_check && self.idle() {
            self.last_rotation_check = Some(now);
            if let Some(row) = self.store().group()?
                && row.stalled.is_none()
                && self.rotation_at.is_none()
                && crate::keys::rotation_due(row.own_leaf_updated_at, now)
            {
                let at = now + crate::keys::rotation_jitter();
                tracing::info!("own leaf is due for rotation");
                self.rotation_at = Some(at);
            }
            if crate::keys::last_resort_due(self.store().last_resort_uploaded_at()?, now) {
                self.push_job(Job::UploadKeyPackages { remaining: None });
            }
            if self.account.is_admin
                && self.group.as_ref().is_some_and(|g| {
                    crate::membership::has_pending_server_removes(
                        g,
                        mls_group::pi_sender_index(g.extensions(), &self.server),
                    )
                })
            {
                self.push_job(Job::CommitServerRemoves);
            }
            // change `dms`: the same upkeep in each DM. Any member commits a
            // DM's pending server Removes; no admin is in a DM.
            for dm in self.store().dms()? {
                if !dm.joined || dm.closed_at.is_some() || dm.state.stalled.is_some() {
                    continue;
                }
                if crate::keys::rotation_due(dm.state.own_leaf_updated_at, now) {
                    self.push_job(Job::RotateIn {
                        group_id: dm.group_id.clone(),
                    });
                }
                if self.dms.get(&dm.group_id).is_some_and(|g| {
                    crate::membership::has_pending_server_removes(
                        g,
                        mls_group::pi_sender_index(g.extensions(), &self.server),
                    )
                }) {
                    self.push_job(Job::CommitServerRemovesIn {
                        group_id: dm.group_id,
                    });
                }
            }
        }
        if let Some(at) = self.rotation_at
            && now >= at
        {
            self.rotation_at = None;
            self.push_job(Job::Rotate);
        }
        if self.last_link_poll.elapsed() >= PENDING_LINK_POLL {
            self.last_link_poll = Instant::now();
            for device in self.store().pending_links()? {
                self.push_job(Job::AddLinked { device });
            }
        }
        Ok(())
    }

    /// Connected, nothing in flight, nothing deferred.
    fn idle(&self) -> bool {
        self.link.is_some() && self.inflight.is_none() && self.deferred.is_empty()
    }

    fn can_run_job(&self) -> bool {
        if !self.idle() || self.jobs.is_empty() {
            return false;
        }
        // After STALE_EPOCH, wait for the commits that moved each group on
        // (unless this device stopped following it).
        self.server_epochs.iter().all(|(group_id, hint)| {
            self.group_ref(group_id)
                .is_none_or(|g| g.epoch().as_u64() >= *hint)
                || self
                    .store()
                    .group_for(group_id)
                    .ok()
                    .flatten()
                    .is_none_or(|r| r.stalled.is_some())
        })
    }

    // --- Frames ---

    async fn on_frame(&mut self, frame: pb::ServerFrame) -> Result<(), EngineError> {
        match frame.frame {
            Some(In::Envelope(envelope)) => self.on_envelope(envelope).await,
            Some(In::SendResult(result)) => self.on_send_result(result).await,
            Some(In::DmClosed(notice)) => {
                self.on_closed(notice.group_id, notice.closed_at, "DM")
                    .await
            }
            Some(In::GroupClosed(notice)) => {
                self.on_closed(notice.group_id, notice.closed_at, "group")
                    .await
            }
            Some(In::GapNotice(notice)) => {
                if let Some(at) = notice.newest_expired_at.as_ref() {
                    let chat = crate::events::ChatId(self.store().chat_of(&notice.group_id)?);
                    self.emit(Event::Gap {
                        newest_expired_at: crate::identity::certificate::from_timestamp(at),
                        chat,
                    });
                }
                Ok(())
            }
            Some(In::LowKeyPackages(low)) => {
                // key-directory: Replenishment signal.
                self.push_job(Job::UploadKeyPackages {
                    remaining: Some(low.remaining),
                });
                Ok(())
            }
            Some(In::RemovedNotice(notice)) => {
                // direct-messages: Inactivity across groups. Its leaf in the
                // DM is stale: it stops following the DM until the Welcome
                // that re-adds it (a fresh start), and sends nothing there.
                if let Some(dm) = self.store().dm_by_group(&notice.group_id)? {
                    if dm.joined && dm.closed_at.is_none() {
                        self.store()
                            .set_stalled_in(&dm.group_id, "removed for inactivity")?;
                    }
                    return Ok(());
                }
                // group-membership: Automatic re-add. This device waits on
                // the stream for the Welcome that re-adds it. A device in no
                // group (it asked to be re-added after an unusable Welcome)
                // was removed from nothing it knows: no notice.
                if self.store().group()?.is_none() {
                    return Ok(());
                }
                if let Some(at) = notice.missed_since.as_ref() {
                    self.emit(Event::RemovedForInactivity {
                        missed_since: crate::identity::certificate::from_timestamp(at),
                    });
                }
                Ok(())
            }
            Some(In::RejoinNotice(notice)) => {
                self.on_rejoin_notice(notice);
                Ok(())
            }
            Some(In::JoinNotice(notice)) => {
                if self.dms.contains_key(&notice.group_id) {
                    self.on_dm_join_notice(notice);
                } else {
                    self.on_join_notice(notice);
                }
                Ok(())
            }
            Some(In::Heartbeat(_)) | None => Ok(()),
        }
    }

    /// group-membership: Automatic re-add. A device of this user (or, on
    /// admin devices, of anyone) is back: re-add it with no user action.
    fn on_rejoin_notice(&mut self, notice: pb::RejoinNotice) {
        let Some(device) = notice.device else {
            return;
        };
        let Some(user) = device.user_id.as_ref().map(UserId::from_pb) else {
            return;
        };
        let Ok(device_id) = DeviceId::try_from(device.device_id.as_slice()) else {
            return;
        };
        if device_id == self.account.device_id {
            return;
        }
        if user != self.user() && !self.account.is_admin {
            return;
        }
        let seen = self
            .group
            .as_ref()
            .and_then(|g| crate::membership::leaf_fingerprint(g, &device_id))
            .map(|(_, key)| key);
        tracing::info!(user = %user, "rejoin notice; re-adding the device");
        self.push_job(Job::Readd {
            user,
            device: device_id,
            seen,
        });
    }

    /// group-membership: Automatic add of new users. On admin devices: a new
    /// user's device waits for its first add; add it with no user action.
    fn on_join_notice(&mut self, notice: pb::JoinNotice) {
        if !self.account.is_admin {
            return;
        }
        let Some(device) = notice.device else {
            return;
        };
        let Some(user) = device.user_id.as_ref().map(UserId::from_pb) else {
            return;
        };
        let Ok(device_id) = DeviceId::try_from(device.device_id.as_slice()) else {
            return;
        };
        if self
            .group
            .as_ref()
            .is_some_and(|g| crate::mls::verify::leaf_of(g, &device_id).is_some())
        {
            return;
        }
        tracing::info!(user = %user, "join notice; adding the new device");
        self.push_job(Job::AddNew {
            user,
            device: device_id,
        });
    }

    /// direct-messages: DM members kept complete. A device of either user
    /// waits to be added to this DM (its peer at first, a new or upgraded
    /// device, or one back from inactivity): add it with no user action.
    fn on_dm_join_notice(&mut self, notice: pb::JoinNotice) {
        let Some(device) = notice.device else {
            return;
        };
        let Some(user) = device.user_id.as_ref().map(UserId::from_pb) else {
            return;
        };
        let Ok(device_id) = DeviceId::try_from(device.device_id.as_slice()) else {
            return;
        };
        if device_id == self.account.device_id {
            return;
        }
        tracing::info!(user = %user, "DM join notice; adding the device");
        self.push_job(Job::AddToDm {
            group_id: notice.group_id,
            user,
            device: device_id,
        });
    }

    /// direct-messages: DM closed on user removal. The peer was removed: the
    /// chat keeps its history, read-only; own messages still sending there
    /// fail. Acknowledged with `server_seq` 0 so the server stops telling.
    /// `what` says whether it was a "DM" or a named "group" (multi-group:
    /// Closing a named group).
    async fn on_closed(
        &mut self,
        group_id: Vec<u8>,
        closed_at: Option<prost_types::Timestamp>,
        what: &str,
    ) -> Result<(), EngineError> {
        let notice = pb::DmClosed {
            group_id,
            closed_at,
        };
        if let Some(dm) = self.store().dm_by_group(&notice.group_id)?
            && dm.closed_at.is_none()
        {
            let at = notice
                .closed_at
                .as_ref()
                .map(crate::identity::certificate::from_timestamp)
                .unwrap_or_else(|| self.now());
            self.store().close_dm(&notice.group_id, at)?;
            if let Some(mut group) = self.dms.remove(&notice.group_id) {
                let _ = group.delete(self.store().mls().storage());
            }
            for message in self.store().outgoing_in_flight()? {
                if message.chat == dm.chat {
                    let reason = format!("the {what} was closed");
                    self.store().mark_failed(message.id, &reason)?;
                    self.emit(Event::SendStatus {
                        id: message.id,
                        status: crate::events::SendStatus::Failed { reason },
                    });
                }
            }
            tracing::info!(what, "a chat was closed");
            self.emit(Event::ChatsChanged);
        }
        self.ack(&notice.group_id, 0).await
    }

    async fn on_envelope(&mut self, envelope: pb::Envelope) -> Result<(), EngineError> {
        let kind = Kind::try_from(envelope.kind).unwrap_or(Kind::Unspecified);
        let waiting = self.inflight.as_ref().is_some_and(|f| f.accepted.is_none());
        let held = !self.deferred.is_empty() && !self.draining;
        if kind == Kind::Welcome {
            // The Welcome this device already joined from, delivered again
            // because its ACK was lost to a crash or a drop (client-runtime:
            // Local message store, "Crash before ack"): its KeyPackage is used
            // up, so staging it again would fail and ask for a needless
            // re-add. A real re-add's Welcome always comes later.
            if self
                .store()
                .group_for(&envelope.group_id)?
                .is_some_and(|row| envelope.server_seq <= row.processed_seq)
            {
                return self.ack(&envelope.group_id, envelope.server_seq).await;
            }
            if held || waiting {
                self.deferred.push_back(envelope);
                return Ok(());
            }
            return self.on_welcome(envelope).await;
        }
        let Some(row) = self.store().group_for(&envelope.group_id)? else {
            // Not in that group (yet): nothing here is for this device.
            return self.ack(&envelope.group_id, envelope.server_seq).await;
        };
        if envelope.server_seq <= row.processed_seq {
            return self.ack(&envelope.group_id, envelope.server_seq).await;
        }
        if let Some(accepted_seq) = self.inflight.as_ref().and_then(|f| f.accepted)
            && envelope.server_seq > accepted_seq
        {
            self.merge_own().await?;
        }
        if held || waiting {
            tracing::debug!(
                server_seq = envelope.server_seq,
                "holding an envelope while the own commit waits"
            );
            self.deferred.push_back(envelope);
            return Ok(());
        }
        if row.stalled.is_some() {
            self.store()
                .set_processed_seq_in(&envelope.group_id, envelope.server_seq)?;
            return self.ack(&envelope.group_id, envelope.server_seq).await;
        }
        match kind {
            Kind::Commit => self.on_commit(envelope).await,
            Kind::Proposal => self.on_proposal(envelope).await,
            Kind::Application => self.on_application(envelope).await,
            Kind::Welcome | Kind::Unspecified => {
                self.store()
                    .set_processed_seq_in(&envelope.group_id, envelope.server_seq)?;
                self.ack(&envelope.group_id, envelope.server_seq).await
            }
        }
    }

    /// Envelopes held while an own commit waited, in order.
    async fn process_deferred(&mut self) -> Result<(), EngineError> {
        if self.draining {
            return Ok(());
        }
        self.draining = true;
        let mut result = Ok(());
        while self.inflight.as_ref().is_none_or(|f| f.accepted.is_some()) {
            let Some(envelope) = self.deferred.pop_front() else {
                break;
            };
            result = Box::pin(self.on_envelope(envelope)).await;
            if result.is_err() {
                break;
            }
        }
        self.draining = false;
        result
    }

    fn reload_group(&mut self) -> Result<(), EngineError> {
        self.group = match self.store().group()? {
            Some(row) => mls_group::load(self.store().mls(), &row.group_id)?,
            None => None,
        };
        Ok(())
    }

    fn parse(bytes: &[u8]) -> Option<ProtocolMessage> {
        MlsMessageIn::tls_deserialize_exact(bytes)
            .ok()?
            .try_into_protocol_message()
            .ok()
    }

    /// This device stops following the group: it refused something the server
    /// sequenced.
    async fn stall(&mut self, envelope: &pb::Envelope, why: String) -> Result<(), EngineError> {
        tracing::warn!(server_seq = envelope.server_seq, why = %why, "refused a commit");
        self.store().transaction(|| {
            self.store().set_stalled_in(&envelope.group_id, &why)?;
            self.store()
                .set_processed_seq_in(&envelope.group_id, envelope.server_seq)
        })?;
        self.emit(Event::CommitRefused { reason: why });
        self.ack(&envelope.group_id, envelope.server_seq).await
    }

    /// A commit from another member: check it, then merge it.
    async fn on_commit(&mut self, envelope: pb::Envelope) -> Result<(), EngineError> {
        let Some(mut group) = self.take_group(&envelope.group_id) else {
            return self
                .stall(&envelope, "a commit arrived but no group is loaded".into())
                .await;
        };
        let result = self.on_commit_in(&mut group, &envelope).await;
        if self.group_ref(&envelope.group_id).is_none() {
            self.put_group(group);
        }
        result
    }

    async fn on_commit_in(
        &mut self,
        group: &mut MlsGroup,
        envelope: &pb::Envelope,
    ) -> Result<(), EngineError> {
        let Some(message) = Self::parse(&envelope.ciphertext) else {
            return self.stall(envelope, "a commit does not parse".into()).await;
        };
        if group.pending_commit().is_some() {
            group
                .clear_pending_commit(self.store().mls().storage())
                .map_err(|e| MlsError::new("clearing a pending commit", e))?;
        }
        let processed = match group.process_message(self.store().mls(), message) {
            Ok(processed) => processed,
            Err(error) => {
                return self
                    .stall(
                        envelope,
                        format!("a commit could not be processed: {error}"),
                    )
                    .await;
            }
        };
        let Sender::Member(committer) = *processed.sender() else {
            return self
                .stall(envelope, "a commit from a non-member".into())
                .await;
        };
        let ProcessedMessageContent::StagedCommitMessage(staged) = processed.into_content() else {
            return self
                .stall(envelope, "a COMMIT envelope without a commit".into())
                .await;
        };
        let needed = verify::users_in_commit(&staged);
        let mut pinned = self.resolve_users(needed.into_keys()).await?;
        let server_index = mls_group::pi_sender_index(group.extensions(), &self.server);
        // key-directory: Clients verify the identity chain. An admin commit
        // that adds a device certified by a user's new identity key (a reset
        // user) is accepted once the server confirms the new key; the key is
        // re-pinned and the UI warned. From anyone else it is refused.
        let mut repinned: Vec<(UserId, Vec<u8>)> = Vec::new();
        let review = loop {
            let review = verify::review_commit(
                group,
                &staged,
                committer,
                &self.account.server_id,
                server_index,
                &|user| pinned.get(user).cloned(),
            );
            let problem = match review {
                Ok(review) => break review,
                Err(problem) => problem,
            };
            let user = verify::key_lookup_allowed(group, committer, &problem)
                .filter(|user| !repinned.iter().any(|(done, _)| done == user));
            let new_key = match user {
                Some(user) => self
                    .changed_identity_key(&user)
                    .await
                    .map(|key| (user, key)),
                None => None,
            };
            let Some((user, key)) = new_key else {
                self.note_key_change(&problem).await;
                return self.stall(envelope, problem.to_string()).await;
            };
            tracing::info!(user = %user, "an admin commit uses a new identity key; checking it");
            pinned.insert(user.clone(), key.clone());
            repinned.push((user, key));
        };
        let seq = envelope.server_seq;
        let now = self.now();
        let by = verify::member_claim(group, committer)
            .ok()
            .map(|claim| claim.claimed.user);
        let merged = self
            .store()
            .transaction(|| -> Result<Vec<Member>, EngineError> {
                for (user, key) in &repinned {
                    self.store()
                        .repin_identity_key(&user.to_string(), key, now)?;
                }
                group
                    .merge_staged_commit(self.store().mls(), *staged)
                    .map_err(|e| MlsError::new("merging a commit", e))?;
                self.store().set_processed_seq_in(&envelope.group_id, seq)?;
                Ok(automatic_changes(self.store(), &review, now)?)
            });
        let automatic = match merged {
            Ok(automatic) => automatic,
            Err(error) => {
                if let Some(reloaded) = mls_group::load(self.store().mls(), &envelope.group_id)? {
                    *group = reloaded;
                }
                return Err(error);
            }
        };
        for (user, _) in repinned {
            self.identity_key_changed(user);
        }
        tracing::info!(
            server_seq = seq,
            epoch = group.epoch().as_u64(),
            added = review.added.len(),
            removed = review.removed.len(),
            "merged a commit"
        );
        if mls_group::kind_of(group.extensions()) != protocol::group_membership::GroupKind::System {
            if !review.added.is_empty() || !review.removed.is_empty() {
                self.emit(Event::ChatsChanged);
            }
            self.named_devices_added(&envelope.group_id, &review);
        } else {
            self.members_changed(&review, by, automatic);
        }
        self.ack(&envelope.group_id, seq).await
    }

    /// A proposal (the server's inactivity Removes): store it so the next commit
    /// includes it.
    async fn on_proposal(&mut self, envelope: pb::Envelope) -> Result<(), EngineError> {
        let seq = envelope.server_seq;
        let server = self.server.clone();
        let is_admin = self.account.is_admin;
        let side = self
            .dms
            .get(&envelope.group_id)
            .map(|g| mls_group::kind_of(g.extensions()));
        let is_dm = side == Some(protocol::group_membership::GroupKind::Dm);
        let is_named = side == Some(protocol::group_membership::GroupKind::Named);
        let Some(group) = group_in(&mut self.group, &mut self.dms, &envelope.group_id) else {
            return self.ack(&envelope.group_id, seq).await;
        };
        let store = Arc::clone(&self.shared.store);
        let outcome = store.transaction(|| -> Result<bool, EngineError> {
            let mut from_server = false;
            if let Some(message) = Self::parse(&envelope.ciphertext) {
                match group.process_message(store.mls(), message) {
                    Ok(processed) => {
                        let server_index = mls_group::pi_sender_index(group.extensions(), &server);
                        from_server = matches!(processed.sender(), Sender::External(index)
                            if server_index.is_some_and(|p| *index == SenderExtensionIndex::new(p)));
                        if let ProcessedMessageContent::ProposalMessage(proposal) =
                            processed.into_content()
                        {
                            group
                                .store_pending_proposal(store.mls().storage(), *proposal)
                                .map_err(|e| MlsError::new("storing a proposal", e))?;
                        }
                    }
                    // A proposal for an epoch this device has left (its own
                    // commit moved on) has nothing left to do.
                    Err(error) => tracing::debug!(%error, "skipped a proposal"),
                }
            }
            store.set_processed_seq_in(&envelope.group_id, seq)?;
            Ok(from_server)
        });
        let from_server = match outcome {
            Ok(from_server) => from_server,
            Err(error) => {
                self.reload_group_in(&envelope.group_id)?;
                return Err(error);
            }
        };
        if from_server && (is_dm || (is_named && is_admin)) {
            // direct-messages: Inactivity across groups. No admin is in a
            // DM, so its members commit the server's Removes; in a named
            // group the admin does, as in the lobby.
            self.push_job(Job::CommitServerRemovesIn {
                group_id: envelope.group_id.clone(),
            });
        } else if from_server && is_admin {
            // group-membership: Inactive devices are kept out. Admin devices
            // commit the server's pending Removes.
            self.push_job(Job::CommitServerRemoves);
        }
        self.ack(&envelope.group_id, seq).await
    }

    /// An application message: decrypt and store it before the ack, then
    /// tell the UI (client-runtime: Local message store; Events in order).
    async fn on_application(&mut self, envelope: pb::Envelope) -> Result<(), EngineError> {
        let seq = envelope.server_seq;
        let store = Arc::clone(&self.shared.store);
        let now = self.shared.clock.now();
        let own_user = UserId::new(
            self.account.username.clone(),
            self.account.server_id.clone(),
        );
        let Some(group) = group_in(&mut self.group, &mut self.dms, &envelope.group_id) else {
            return self.ack(&envelope.group_id, seq).await;
        };
        let admin = mls_group::admin_user_id(group.extensions());
        let result = store.transaction(|| -> Result<_, EngineError> {
            let received = crate::messaging::receive_application(
                &store,
                group,
                &envelope,
                admin.as_deref(),
                &own_user,
                now,
            )?;
            store.set_processed_seq_in(&envelope.group_id, seq)?;
            Ok(received)
        });
        let received = match result {
            Ok(received) => received,
            Err(error) => {
                self.reload_group_in(&envelope.group_id)?;
                return Err(error);
            }
        };
        self.emit_received(received);
        self.ack(&envelope.group_id, seq).await
    }

    /// A Welcome: check every leaf, then join (replacing an old group after
    /// a re-add).
    async fn on_welcome(&mut self, envelope: pb::Envelope) -> Result<(), EngineError> {
        let seq = envelope.server_seq;
        let welcome = match MlsMessageIn::tls_deserialize_exact(envelope.ciphertext.as_slice())
            .map(MlsMessageIn::extract)
        {
            Ok(MlsMessageBodyIn::Welcome(welcome)) => welcome,
            _ => {
                tracing::warn!(server_seq = seq, "a WELCOME envelope without a Welcome");
                return self.ack(&envelope.group_id, seq).await;
            }
        };
        let staged = StagedWelcome::build_from_welcome(
            self.store().mls(),
            &mls_group::join_config(),
            welcome,
        )
        .and_then(|builder| builder.replace_old_group().build());
        let staged = match staged {
            Ok(staged) => staged,
            Err(error) => {
                tracing::warn!(server_seq = seq, %error, "cannot use a Welcome");
                let reason = format!("a Welcome could not be used: {error}");
                return self.refuse_welcome(&envelope.group_id, seq, reason).await;
            }
        };
        // change `dms`: a DM's Welcome names its two users instead of an
        // admin.
        let group_kind = mls_group::kind_of(staged.group_context().extensions());
        let is_dm = group_kind == protocol::group_membership::GroupKind::Dm;
        let is_named = group_kind == protocol::group_membership::GroupKind::Named;
        let dm_peer: Option<UserId>;
        let needed = verify::users_in_welcome(staged.members());
        let mut pinned = self.resolve_users(needed.into_keys()).await?;
        // key-directory: Clients verify the identity chain. A leaf that does
        // not verify against its user's pinned key (a user reset while this
        // device was away) is checked again with the key the server has now,
        // whoever signed the Welcome; the server vouches for every key a device
        // first sees in a Welcome anyway (owner 2026-11-11, task 6.4).
        let mut repinned: Vec<(UserId, Vec<u8>)> = Vec::new();
        loop {
            let checked = if is_dm {
                verify::check_dm_welcome(
                    staged.members(),
                    staged.group_context().extensions(),
                    &self.user(),
                    &self.account.server_id,
                    &self.server,
                    &|user| pinned.get(user).cloned(),
                )
                .map(Some)
            } else {
                verify::check_welcome(
                    staged.members(),
                    staged.group_context().extensions(),
                    &self.account.server_id,
                    &self.server,
                    &|user| pinned.get(user).cloned(),
                )
                .map(|()| None)
            };
            let problem = match checked {
                Ok(peer) => {
                    dm_peer = peer;
                    break;
                }
                Err(problem) => problem,
            };
            let user = verify::welcome_key_lookup(&problem)
                .filter(|user| !repinned.iter().any(|(done, _)| done == user));
            let new_key = match user {
                Some(user) => self
                    .changed_identity_key(&user)
                    .await
                    .map(|key| (user, key)),
                None => None,
            };
            let Some((user, key)) = new_key else {
                self.note_key_change(&problem).await;
                tracing::warn!(server_seq = seq, %problem, "refused a Welcome");
                if is_dm || is_named {
                    // Nothing to ask the server for: a DM or named group is
                    // not worth a re-add of this device to the lobby.
                    return self.ack(&envelope.group_id, seq).await;
                }
                return self
                    .refuse_welcome(&envelope.group_id, seq, problem.to_string())
                    .await;
            };
            tracing::info!(user = %user, "a Welcome uses a new identity key; checking it");
            pinned.insert(user.clone(), key.clone());
            repinned.push((user, key));
        }
        if let Some(peer) = dm_peer {
            return self.join_dm(&envelope, staged, peer, repinned).await;
        }
        if is_named {
            return self.join_named(&envelope, staged, repinned).await;
        }
        let rejoined = self.store().group()?.is_some();
        let now = self.now();
        let store = Arc::clone(&self.shared.store);
        let joined = store.transaction(|| -> Result<MlsGroup, EngineError> {
            for (user, key) in &repinned {
                store.repin_identity_key(&user.to_string(), key, now)?;
            }
            let group = staged
                .into_group(store.mls())
                .map_err(|e| MlsError::new("joining from a Welcome", e))?;
            store.set_group(group.group_id().as_slice(), now, seq)?;
            store.set_processed_seq(seq)?;
            Ok(group)
        });
        match joined {
            Ok(group) => {
                tracing::info!(
                    server_seq = seq,
                    epoch = group.epoch().as_u64(),
                    "joined the group"
                );
                self.group = Some(group);
                self.rotation_at = None;
            }
            Err(error) => {
                self.reload_group()?;
                return Err(error);
            }
        }
        self.emit(if rejoined {
            Event::Rejoined
        } else {
            Event::JoinedGroup
        });
        for (user, _) in repinned {
            self.identity_key_changed(user);
        }
        // qa-fixes D5: no notices for the commit that added this device.
        self.emit(Event::MembersChanged {
            added: Vec::new(),
            removed: Vec::new(),
            by: None,
            automatic: Vec::new(),
        });
        self.ack(&envelope.group_id, seq).await
    }

    /// Joins a DM from its checked Welcome (direct-messages: Several groups
    /// in the core): only this DM's state changes, the main group's stays.
    async fn join_dm(
        &mut self,
        envelope: &pb::Envelope,
        staged: StagedWelcome,
        peer: UserId,
        repinned: Vec<(UserId, Vec<u8>)>,
    ) -> Result<(), EngineError> {
        let seq = envelope.server_seq;
        let now = self.now();
        let store = Arc::clone(&self.shared.store);
        let joined = store.transaction(|| -> Result<MlsGroup, EngineError> {
            for (user, key) in &repinned {
                store.repin_identity_key(&user.to_string(), key, now)?;
            }
            let group = staged
                .into_group(store.mls())
                .map_err(|e| MlsError::new("joining a DM from a Welcome", e))?;
            store.set_dm_joined(group.group_id().as_slice(), &peer.to_string(), now, seq)?;
            Ok(group)
        });
        let group = match joined {
            Ok(group) => group,
            Err(error) => {
                self.reload_group_in(&envelope.group_id)?;
                return Err(error);
            }
        };
        tracing::info!(
            server_seq = seq,
            epoch = group.epoch().as_u64(),
            "joined a DM"
        );
        self.dms.insert(group.group_id().as_slice().to_vec(), group);
        for (user, _) in repinned {
            self.identity_key_changed(user);
        }
        self.emit(Event::ChatsChanged);
        self.ack(&envelope.group_id, seq).await
    }

    /// Joins a named group from its checked Welcome (multi-group: Named group
    /// membership): only this group's state changes, the lobby's stays.
    async fn join_named(
        &mut self,
        envelope: &pb::Envelope,
        staged: StagedWelcome,
        repinned: Vec<(UserId, Vec<u8>)>,
    ) -> Result<(), EngineError> {
        let seq = envelope.server_seq;
        let now = self.now();
        let store = Arc::clone(&self.shared.store);
        let joined = store.transaction(|| -> Result<MlsGroup, EngineError> {
            for (user, key) in &repinned {
                store.repin_identity_key(&user.to_string(), key, now)?;
            }
            let group = staged
                .into_group(store.mls())
                .map_err(|e| MlsError::new("joining a named group from a Welcome", e))?;
            store.set_named_joined(group.group_id().as_slice(), now, seq)?;
            Ok(group)
        });
        let group = match joined {
            Ok(group) => group,
            Err(error) => {
                self.reload_group_in(&envelope.group_id)?;
                return Err(error);
            }
        };
        tracing::info!(
            server_seq = seq,
            epoch = group.epoch().as_u64(),
            "joined a named group"
        );
        self.dms.insert(group.group_id().as_slice().to_vec(), group);
        for (user, _) in repinned {
            self.identity_key_changed(user);
        }
        self.emit(Event::ChatsChanged);
        self.ack(&envelope.group_id, seq).await
    }

    /// multi-group: after a commit added devices to the named group
    /// `group_id`, an admin device re-sends the group's name so they learn it
    /// (message-content: Group settings travel encrypted).
    pub(super) fn named_devices_added(&self, group_id: &[u8], review: &verify::CommitReview) {
        if review.added.is_empty() || !self.account.is_admin {
            return;
        }
        if let Err(error) = self.resend_named_name(group_id) {
            tracing::error!(%error, "cannot queue the re-send of a group's name");
        }
    }

    /// A Welcome this device cannot use: say why, ACK it, and ask the server to
    /// be added again (group-membership: Re-add after an unusable Welcome).
    async fn refuse_welcome(
        &mut self,
        group_id: &[u8],
        seq: u64,
        reason: String,
    ) -> Result<(), EngineError> {
        self.emit(Event::CommitRefused {
            reason: reason.clone(),
        });
        self.ack(group_id, seq).await?;
        self.push_job(Job::RequestReadd {
            refused_seq: seq,
            reason,
        });
        Ok(())
    }

    // --- Identity keys (key-directory: Clients verify the identity chain) ---

    /// The pinned identity key of each user, pinning on first sight with a
    /// key looked up with `Keys.GetIdentityKeys` (key-directory: Identity key
    /// lookup; design D5 clarification "Learning identity keys").
    async fn resolve_users(
        &self,
        users: impl IntoIterator<Item = UserId>,
    ) -> Result<BTreeMap<UserId, Vec<u8>>, EngineError> {
        let mut keys = BTreeMap::new();
        let mut unknown = Vec::new();
        for user in users {
            match self.store().pinned_identity_key(&user.to_string())? {
                Some(pinned) => {
                    keys.insert(user, pinned.key);
                }
                None => unknown.push(user),
            }
        }
        if unknown.is_empty() {
            return Ok(keys);
        }
        let looked_up = match self.session() {
            Ok(session) => crate::keys::identity_keys(&session, &unknown).await,
            Err(error) => return Err(error),
        };
        match looked_up {
            Ok(found) => {
                for (user, key) in found {
                    if self
                        .store()
                        .pin_identity_key(&user.to_string(), &key, self.now())?
                        == PinOutcome::Pinned
                    {
                        keys.insert(user, key);
                    }
                }
            }
            Err(error) => tracing::warn!(%error, "cannot look up identity keys"),
        }
        Ok(keys)
    }

    /// The identity key the server has now for `user`, if it differs from the
    /// pinned one.
    async fn changed_identity_key(&self, user: &UserId) -> Option<Vec<u8>> {
        let session = self.session().ok()?;
        let found = match crate::keys::identity_keys(&session, std::slice::from_ref(user)).await {
            Ok(found) => found,
            Err(error) => {
                tracing::warn!(user = %user, %error, "cannot look up an identity key");
                return None;
            }
        };
        let key = found.get(user)?.clone();
        let pinned = self.store().pinned_identity_key(&user.to_string()).ok()??;
        (pinned.key != key).then_some(key)
    }

    /// After a certificate failed against a pinned key in something this
    /// device refuses, checks whether the user's key changed and, if so,
    /// keeps the new key as pending and warns the UI.
    async fn note_key_change(&self, problem: &LeafProblem) {
        let LeafProblem::Unverified(user) = problem else {
            return;
        };
        let Some(user) = parse_user_id(user) else {
            return;
        };
        let Some(key) = self.changed_identity_key(&user).await else {
            return;
        };
        if let Ok(PinOutcome::Changed) =
            self.store()
                .pin_identity_key(&user.to_string(), &key, self.now())
        {
            self.identity_key_changed(user);
        }
    }

    /// Pins the identity key a fetch returned for `user`. A key that differs
    /// from the pinned one warns the UI and refuses (key-directory: Clients
    /// verify the identity chain).
    fn check_pin(&self, user: &UserId, key: &[u8]) -> Result<(), ClientError> {
        match self
            .store()
            .pin_identity_key(&user.to_string(), key, self.now())?
        {
            PinOutcome::Pinned | PinOutcome::Matches => Ok(()),
            PinOutcome::Changed => {
                self.identity_key_changed(user.clone());
                Err(ClientError::IdentityKeyChanged(user.clone()))
            }
        }
    }

    // --- Own commits ---

    /// Records and sends an own commit staged in `group` for `job`.
    async fn send_commit(
        &mut self,
        job: Job,
        staged: crate::membership::StagedOwnCommit,
        recipients: Vec<pb::DeviceRef>,
    ) -> Result<(), EngineError> {
        let group_id = match job.dm_group() {
            Some(group_id) => group_id.to_vec(),
            None => match self.main_id() {
                Some(group_id) => group_id,
                None => {
                    job.fail(ClientError::NoGroup);
                    return Ok(());
                }
            },
        };
        if self.group_ref(&group_id).is_none() {
            job.fail(ClientError::NoGroup);
            return Ok(());
        }
        let msg_id = random_msg_id()?;
        let epoch = staged.epoch;
        let encoded =
            match crate::membership::commit_envelope(msg_id, &group_id, staged, recipients) {
                Ok(encoded) => encoded,
                Err(error) => {
                    self.clear_pending_in(&group_id)?;
                    job.fail(error);
                    return Ok(());
                }
            };
        let row = OutgoingCommitRow {
            msg_id,
            envelope: encoded,
            purpose: job.purpose(),
        };
        self.store().set_outgoing_commit(&row)?;
        tracing::info!(epoch, purpose = %row.purpose, "sending a commit");
        self.inflight = Some(InFlight {
            row,
            job,
            group_id,
            sent: false,
            accepted: None,
        });
        self.resend_inflight().await
    }

    /// Sends the own commit in flight on the current link, if not yet sent.
    async fn resend_inflight(&mut self) -> Result<(), EngineError> {
        let Some(inflight) = self.inflight.as_ref() else {
            return Ok(());
        };
        if inflight.sent || self.link.is_none() {
            return Ok(());
        }
        if inflight.accepted.is_some() {
            return self.merge_own().await;
        }
        let envelope = pb::Envelope::decode(inflight.row.envelope.as_slice())
            .map_err(|e| StoreError::Inconsistent(format!("stored commit envelope: {e}")))?;
        self.send(Out::Envelope(envelope)).await?;
        if let Some(inflight) = self.inflight.as_mut() {
            inflight.sent = true;
        }
        Ok(())
    }

    fn clear_pending_in(&mut self, group_id: &[u8]) -> Result<(), EngineError> {
        let store = Arc::clone(&self.shared.store);
        if let Some(group) = self.group_mut(group_id) {
            store.transaction(|| -> Result<(), EngineError> {
                group
                    .clear_pending_commit(store.mls().storage())
                    .map_err(|e| MlsError::new("clearing a pending commit", e))?;
                store.clear_outgoing_commit()?;
                Ok(())
            })?;
        } else {
            store.clear_outgoing_commit()?;
        }
        Ok(())
    }

    async fn on_send_result(&mut self, result: pb::SendResult) -> Result<(), EngineError> {
        if self.on_app_result(&result)? {
            return Ok(());
        }
        let ours = self
            .inflight
            .as_ref()
            .is_some_and(|f| f.row.msg_id.as_slice() == result.msg_id.as_slice());
        if !ours {
            return Ok(());
        }
        match result.result {
            Some(pb::send_result::Result::Accepted(accepted)) => {
                // Everything below the commit was delivered before this
                // result: read what is held below it in the old epoch (the
                // first envelope after it merges), then merge.
                if let Some(inflight) = self.inflight.as_mut() {
                    inflight.accepted = Some(accepted.server_seq);
                }
                self.process_deferred().await?;
                self.merge_own().await?;
                self.process_deferred().await
            }
            Some(pb::send_result::Result::Rejected(rejected)) => {
                let group_id = self
                    .inflight
                    .as_ref()
                    .map(|f| f.group_id.clone())
                    .unwrap_or_default();
                self.clear_pending_in(&group_id)?;
                let Some(inflight) = self.inflight.take() else {
                    return Ok(());
                };
                let reason = pb::RejectionReason::try_from(rejected.reason)
                    .unwrap_or(pb::RejectionReason::Unspecified);
                if reason == pb::RejectionReason::StaleEpoch {
                    // message-delivery: Strict epoch rule. Catch up, then
                    // build the commit again.
                    tracing::info!(
                        current_epoch = rejected.current_epoch,
                        "commit was stale; retrying after catching up"
                    );
                    let hint = self.server_epochs.entry(group_id).or_default();
                    *hint = (*hint).max(rejected.current_epoch);
                    self.jobs.push_front(inflight.job);
                } else {
                    tracing::warn!(reason = reason.as_str_name(), "commit rejected");
                    inflight
                        .job
                        .fail(ClientError::Rejected(reason.as_str_name().to_owned()));
                }
                self.process_deferred().await
            }
            None => Ok(()),
        }
    }

    /// The server accepted the own commit: merge it and finish its job.
    async fn merge_own(&mut self) -> Result<(), EngineError> {
        let Some(inflight) = self.inflight.take() else {
            return Ok(());
        };
        let now = self.now();
        let store = Arc::clone(&self.shared.store);
        let server = self.server.clone();
        let group_id = inflight.group_id.clone();
        let is_dm = self.dms.contains_key(&group_id);
        let merged = match self.group_mut(&group_id) {
            Some(group) => store.transaction(|| -> Result<_, EngineError> {
                let server_index = mls_group::pi_sender_index(group.extensions(), &server);
                let review = group
                    .pending_commit()
                    .map(|staged| verify::own_commit_changes(group, staged, server_index))
                    .unwrap_or_default();
                let had_path = group
                    .pending_commit()
                    .is_some_and(|c| c.update_path_leaf_node().is_some());
                group
                    .merge_pending_commit(store.mls())
                    .map_err(|e| MlsError::new("merging the own commit", e))?;
                store.clear_outgoing_commit()?;
                if had_path {
                    store.set_own_leaf_updated_in(&group_id, now)?;
                }
                let automatic = automatic_changes(&store, &review, now)?;
                Ok((review, automatic))
            }),
            None => Ok((verify::CommitReview::default(), Vec::new())),
        };
        let (review, automatic) = match merged {
            Ok(merged) => merged,
            Err(error) => {
                self.reload_group_in(&group_id)?;
                inflight.job.fail(ClientError::from(error));
                return Ok(());
            }
        };
        if let Some(group) = self.group_ref(&group_id) {
            tracing::info!(epoch = group.epoch().as_u64(), "own commit merged");
        }
        if is_dm {
            if !review.added.is_empty() || !review.removed.is_empty() {
                self.emit(Event::ChatsChanged);
            }
            self.named_devices_added(&group_id, &review);
        } else {
            self.members_changed(&review, Some(self.user()), automatic);
        }
        self.finish(inflight.job).await;
        Ok(())
    }

    /// Tells the UI what a merged commit changed (client-runtime: Events in
    /// order) and, when it added devices, queues the re-sends they need
    /// (qa-fixes D2).
    fn members_changed(
        &self,
        review: &verify::CommitReview,
        by: Option<UserId>,
        automatic: Vec<Member>,
    ) {
        let members = |list: &[(UserId, DeviceId)]| -> Vec<Member> {
            list.iter()
                .map(|(user, device)| Member {
                    user: user.clone(),
                    device: *device,
                })
                .collect()
        };
        self.emit(Event::MembersChanged {
            added: members(&review.added),
            removed: members(&review.removed),
            by,
            automatic,
        });
        if !review.added.is_empty()
            && let Err(error) = self.resend_state()
        {
            tracing::error!(%error, "cannot queue the re-sends after an add");
        }
    }

    /// A user's identity key changed (a reset user): drop their stored
    /// profile, so they show as the username until the new identity sets one
    /// (message-content: User-owned profiles; qa-fixes D3), and warn the UI.
    fn identity_key_changed(&self, user: UserId) {
        let dropped = match self.store().delete_profile(&user.to_string()) {
            Ok(dropped) => dropped,
            Err(error) => {
                tracing::error!(%error, "cannot drop a changed user's profile");
                false
            }
        };
        self.emit(Event::IdentityKeyChanged { user: user.clone() });
        if dropped {
            self.emit(Event::ProfileChanged {
                user,
                display_name: None,
                about: None,
            });
        }
    }

    async fn run_next_job(&mut self) -> Result<(), EngineError> {
        let Some(job) = self.jobs.pop_front() else {
            return Ok(());
        };
        self.run_job(job).await
    }

    /// The group, unless this device has none or stalled.
    fn usable_group(&self) -> Result<(), ClientError> {
        let Some(row) = self.store().group()? else {
            return Err(ClientError::NoGroup);
        };
        if let Some(why) = row.stalled {
            return Err(ClientError::Stalled(why));
        }
        if self.group.is_none() {
            return Err(ClientError::NoGroup);
        }
        Ok(())
    }
}
