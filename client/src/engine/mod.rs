//! The engine: one task that owns the connection to the Pi, the `Connect`
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
//! in the next epoch. The Pi delivers every entry below the commit's
//! `server_seq` before `Accepted` (message-delivery: Ordered delivery and
//! catch-up), so on `Accepted` the held envelopes below it are read in the
//! old epoch, then the commit is merged, then the rest follow. On
//! `STALE_EPOCH` the job goes back to the front of the queue and runs again
//! once the device has caught up to the epoch the Pi named.
//!
//! **Incoming.** Strictly in `server_seq` order; each is processed and
//! recorded (with `processed_seq`) in one store transaction and acked after
//! it commits (client-runtime: Local message store). Commits and Welcomes are
//! checked with [`crate::mls::verify`] first.

mod jobs;

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
use protocol::wire_transport::{HEARTBEAT_INTERVAL, PROTOCOL_VERSION, STREAM_IDLE_TIMEOUT};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;

use crate::client::ClientError;
use crate::clock::Clock;
use crate::events::{ConnectionState, DeviceId, Event, EventSender, UserId};
use crate::identity::DeviceKeys;
use crate::keystore::KeyStore;
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
}

/// What every part of the client shares.
pub(crate) struct Shared {
    pub(crate) store: Arc<Store>,
    pub(crate) key_store: Arc<dyn KeyStore>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) events: EventSender,
    /// The engine's current session, for the client's unary calls.
    pub(crate) session: Mutex<Option<Session>>,
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
    pi: ExternalSender,
    commands: mpsc::Receiver<Command>,
    group: Option<MlsGroup>,
    link: Option<Link>,
    backoff: Backoff,
    next_attempt: Instant,
    /// A pin mismatch stops connecting until the client starts again.
    halted: bool,
    jobs: VecDeque<Job>,
    inflight: Option<InFlight>,
    /// Incoming envelopes held while an own commit waits for its result.
    deferred: VecDeque<pb::Envelope>,
    /// The held envelopes are being processed (so one taken from the front
    /// is not held again behind the rest).
    draining: bool,
    /// The epoch the Pi said the group is at (from `STALE_EPOCH`).
    server_epoch: u64,
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
    ) -> Result<Self, EngineError> {
        let pi =
            mls_group::pi_external_sender(&account.pi_sender_key, &account.pi_sender_credential)?;
        let group = match shared.store.group()? {
            Some(row) => mls_group::load(shared.store.mls(), &row.group_id)?,
            None => None,
        };
        let mut engine = Self {
            shared,
            keys,
            account,
            pi,
            commands,
            group,
            link: None,
            backoff: Backoff::new(),
            next_attempt: Instant::now(),
            halted: false,
            jobs: VecDeque::new(),
            inflight: None,
            deferred: VecDeque::new(),
            draining: false,
            server_epoch: 0,
            quiet_since: Instant::now(),
            last_rotation_check: None,
            rotation_at: None,
            last_link_poll: Instant::now(),
        };
        if let Some(row) = engine.shared.store.outgoing_commit()? {
            let pending = engine
                .group
                .as_ref()
                .is_some_and(|g| g.pending_commit().is_some());
            if pending {
                tracing::info!("resending the own commit left in flight");
                let job = Job::recovered(&row.purpose);
                engine.inflight = Some(InFlight {
                    row,
                    job,
                    sent: false,
                    accepted: None,
                });
            } else {
                engine.shared.store.clear_outgoing_commit()?;
            }
        }
        if engine.shared.store.last_resort_uploaded_at()?.is_none() {
            engine.push_job(Job::UploadKeyPackages { remaining: Some(0) });
        }
        Ok(engine)
    }

    fn now(&self) -> SystemTime {
        self.shared.clock.now()
    }

    fn store(&self) -> &Store {
        &self.shared.store
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
                    }
                    Ok(None) => self.drop_link("the server closed the stream"),
                    Err(status) => {
                        let why = format!("stream ended: {status}");
                        self.drop_link(&why);
                    }
                },
                command = self.commands.recv() => match command {
                    Some(Command::Job(job)) => self.push_job(job),
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
                self.link = Some(link);
                self.quiet_since = Instant::now();
                self.last_rotation_check = None;
                tracing::info!("connected");
                self.emit(Event::Connection(ConnectionState::Connected));
                if let Err(error) = self.resend_inflight().await {
                    self.on_error(error);
                }
            }
            Ok(Err(NetError::ServerIdentity)) => {
                self.halted = true;
                self.emit(Event::ServerIdentityError);
                self.emit(Event::Connection(ConnectionState::Disconnected));
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

    /// Pinned TLS, challenge auth, then `Connect` with `Hello`.
    async fn open_link(&self) -> Result<Link, NetError> {
        let conn = Conn::open(&self.account.relay_address, self.account.tls_pin).await?;
        let session = conn
            .authenticate(&self.keys, self.device_ref(), self.now())
            .await?;
        let (tx, frames) = mpsc::channel(64);
        tx.send(pb::ClientFrame {
            frame: Some(Out::Hello(pb::Hello {
                protocol_version: PROTOCOL_VERSION,
            })),
        })
        .await
        .map_err(|_| NetError::Connect("stream closed".into()))?;
        let rx = conn
            .messaging()
            .connect(session.request(ReceiverStream::new(frames))?)
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
        // the first held one), so the Pi delivers them again.
        self.deferred.clear();
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
                    if let Some(link) = self.link.as_mut() {
                        link.session = session;
                    }
                    tracing::debug!("renewed the session token");
                }
                Err(error) => {
                    tracing::warn!(%error, "token renewal failed");
                    self.drop_link("token renewal failed");
                    return Ok(());
                }
            }
        }
        self.timed_checks()?;
        Ok(())
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
                    crate::membership::has_pending_pi_removes(
                        g,
                        mls_group::pi_sender_index(g.extensions(), &self.pi),
                    )
                })
            {
                self.push_job(Job::CommitPiRemoves);
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
        // After STALE_EPOCH, wait for the commits that moved the group on.
        let epoch = self.group.as_ref().map_or(0, |g| g.epoch().as_u64());
        epoch >= self.server_epoch
            || self
                .store()
                .group()
                .ok()
                .flatten()
                .is_some_and(|r| r.stalled.is_some())
    }

    // --- Frames ---

    async fn on_frame(&mut self, frame: pb::ServerFrame) -> Result<(), EngineError> {
        match frame.frame {
            Some(In::Envelope(envelope)) => self.on_envelope(envelope).await,
            Some(In::SendResult(result)) => self.on_send_result(result).await,
            Some(In::GapNotice(notice)) => {
                if let Some(at) = notice.newest_expired_at.as_ref() {
                    self.emit(Event::Gap {
                        newest_expired_at: crate::identity::certificate::from_timestamp(at),
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
                // group-membership: Automatic re-add. This device waits on
                // the stream for the Welcome that re-adds it.
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

    async fn on_envelope(&mut self, envelope: pb::Envelope) -> Result<(), EngineError> {
        let kind = Kind::try_from(envelope.kind).unwrap_or(Kind::Unspecified);
        let waiting = self.inflight.as_ref().is_some_and(|f| f.accepted.is_none());
        let held = !self.deferred.is_empty() && !self.draining;
        if kind == Kind::Welcome {
            if held || waiting {
                self.deferred.push_back(envelope);
                return Ok(());
            }
            return self.on_welcome(envelope).await;
        }
        let Some(row) = self.store().group()? else {
            // Not in the group yet: nothing here is for this device.
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
            self.deferred.push_back(envelope);
            return Ok(());
        }
        if row.stalled.is_some() {
            self.store().set_processed_seq(envelope.server_seq)?;
            return self.ack(&envelope.group_id, envelope.server_seq).await;
        }
        match kind {
            Kind::Commit => self.on_commit(envelope).await,
            Kind::Proposal => self.on_proposal(envelope).await,
            Kind::Application => self.on_application(envelope).await,
            Kind::Welcome | Kind::Unspecified => {
                self.store().set_processed_seq(envelope.server_seq)?;
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

    /// This device stops following the group: it refused something the Pi
    /// sequenced.
    async fn stall(&mut self, envelope: &pb::Envelope, why: String) -> Result<(), EngineError> {
        tracing::warn!(server_seq = envelope.server_seq, why = %why, "refused a commit");
        self.store().transaction(|| {
            self.store().set_stalled(&why)?;
            self.store().set_processed_seq(envelope.server_seq)
        })?;
        self.emit(Event::CommitRefused { reason: why });
        self.ack(&envelope.group_id, envelope.server_seq).await
    }

    /// A commit from another member: check it, then merge it.
    async fn on_commit(&mut self, envelope: pb::Envelope) -> Result<(), EngineError> {
        let Some(mut group) = self.group.take() else {
            return self
                .stall(&envelope, "a commit arrived but no group is loaded".into())
                .await;
        };
        let result = self.on_commit_in(&mut group, &envelope).await;
        if self.group.is_none() {
            self.group = Some(group);
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
        let pi_index = mls_group::pi_sender_index(group.extensions(), &self.pi);
        // key-directory: Clients verify the identity chain. An admin commit
        // that adds a device certified by a user's new identity key (a reset
        // user) is accepted once the Pi confirms the new key; the key is
        // re-pinned and the UI warned. From anyone else it is refused.
        let mut repinned: Vec<(UserId, Vec<u8>)> = Vec::new();
        let review = loop {
            let review = verify::review_commit(
                group,
                &staged,
                committer,
                &self.account.server_id,
                pi_index,
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
        let merged = self.store().transaction(|| -> Result<(), EngineError> {
            for (user, key) in &repinned {
                self.store()
                    .repin_identity_key(&user.to_string(), key, now)?;
            }
            group
                .merge_staged_commit(self.store().mls(), *staged)
                .map_err(|e| MlsError::new("merging a commit", e))?;
            self.store().set_processed_seq(seq)?;
            Ok(())
        });
        if let Err(error) = merged {
            self.reload_group()?;
            return Err(error);
        }
        for (user, _) in repinned {
            self.emit(Event::IdentityKeyChanged { user });
        }
        tracing::info!(
            server_seq = seq,
            epoch = group.epoch().as_u64(),
            added = review.added.len(),
            removed = review.removed.len(),
            "merged a commit"
        );
        self.emit(Event::MembersChanged);
        self.ack(&envelope.group_id, seq).await
    }

    /// A proposal (the Pi's inactivity Removes): store it so the next commit
    /// includes it.
    async fn on_proposal(&mut self, envelope: pb::Envelope) -> Result<(), EngineError> {
        let seq = envelope.server_seq;
        let pi = self.pi.clone();
        let is_admin = self.account.is_admin;
        let Some(group) = self.group.as_mut() else {
            return self.ack(&envelope.group_id, seq).await;
        };
        let store = Arc::clone(&self.shared.store);
        let outcome = store.transaction(|| -> Result<bool, EngineError> {
            let mut from_pi = false;
            if let Some(message) = Self::parse(&envelope.ciphertext) {
                match group.process_message(store.mls(), message) {
                    Ok(processed) => {
                        let pi_index = mls_group::pi_sender_index(group.extensions(), &pi);
                        from_pi = matches!(processed.sender(), Sender::External(index)
                            if pi_index.is_some_and(|p| *index == SenderExtensionIndex::new(p)));
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
            store.set_processed_seq(seq)?;
            Ok(from_pi)
        });
        let from_pi = match outcome {
            Ok(from_pi) => from_pi,
            Err(error) => {
                self.reload_group()?;
                return Err(error);
            }
        };
        if from_pi && is_admin {
            // group-membership: Inactive devices are kept out. Admin devices
            // commit the Pi's pending Removes.
            self.push_job(Job::CommitPiRemoves);
        }
        self.ack(&envelope.group_id, seq).await
    }

    /// An application message: decrypt and store it before the ack. Decoding
    /// the content and telling the UI is task group 6 (messaging).
    async fn on_application(&mut self, envelope: pb::Envelope) -> Result<(), EngineError> {
        let seq = envelope.server_seq;
        let Some(group) = self.group.as_mut() else {
            return self.ack(&envelope.group_id, seq).await;
        };
        let store = Arc::clone(&self.shared.store);
        let now = self.shared.clock.now();
        let result = store.transaction(|| -> Result<(), EngineError> {
            crate::messaging::receive_application(&store, group, &envelope, now)?;
            store.set_processed_seq(seq)?;
            Ok(())
        });
        if let Err(error) = result {
            self.reload_group()?;
            return Err(error);
        }
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
                self.emit(Event::CommitRefused {
                    reason: format!("a Welcome could not be used: {error}"),
                });
                return self.ack(&envelope.group_id, seq).await;
            }
        };
        let needed = verify::users_in_welcome(staged.members());
        let pinned = self.resolve_users(needed.into_keys()).await?;
        let checked = verify::check_welcome(
            staged.members(),
            staged.group_context().extensions(),
            &self.account.server_id,
            &self.pi,
            &|user| pinned.get(user).cloned(),
        );
        if let Err(problem) = checked {
            self.note_key_change(&problem).await;
            tracing::warn!(server_seq = seq, %problem, "refused a Welcome");
            self.emit(Event::CommitRefused {
                reason: problem.to_string(),
            });
            return self.ack(&envelope.group_id, seq).await;
        }
        let rejoined = self.store().group()?.is_some();
        let now = self.now();
        let store = Arc::clone(&self.shared.store);
        let joined = store.transaction(|| -> Result<MlsGroup, EngineError> {
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
        self.emit(Event::MembersChanged);
        self.ack(&envelope.group_id, seq).await
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

    /// The identity key the Pi has now for `user`, if it differs from the
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
            self.emit(Event::IdentityKeyChanged { user });
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
                self.emit(Event::IdentityKeyChanged { user: user.clone() });
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
        let Some(group) = self.group.as_ref() else {
            job.fail(ClientError::NoGroup);
            return Ok(());
        };
        let envelope = pb::Envelope {
            msg_id: random_msg_id()?.to_vec(),
            group_id: group.group_id().as_slice().to_vec(),
            epoch: staged.epoch,
            kind: Kind::Commit as i32,
            ciphertext: staged.commit,
            welcome: staged.welcome.map(|welcome| pb::WelcomeBundle {
                welcome,
                recipients,
            }),
            ..Default::default()
        };
        let encoded = envelope.encode_to_vec();
        if encoded.len() > protocol::wire_transport::MAX_HANDSHAKE_ENVELOPE_BYTES {
            self.clear_pending()?;
            job.fail(ClientError::TooLarge);
            return Ok(());
        }
        let mut msg_id = [0u8; 16];
        msg_id.copy_from_slice(&envelope.msg_id);
        let row = OutgoingCommitRow {
            msg_id,
            envelope: encoded,
            purpose: job.purpose(),
        };
        self.store().set_outgoing_commit(&row)?;
        tracing::info!(epoch = envelope.epoch, purpose = %row.purpose, "sending a commit");
        self.inflight = Some(InFlight {
            row,
            job,
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

    fn clear_pending(&mut self) -> Result<(), EngineError> {
        let store = Arc::clone(&self.shared.store);
        if let Some(group) = self.group.as_mut() {
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
        let ours = self
            .inflight
            .as_ref()
            .is_some_and(|f| f.row.msg_id.as_slice() == result.msg_id.as_slice());
        if !ours {
            // Results for application messages: task group 6 (messaging).
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
                self.clear_pending()?;
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
                    self.server_epoch = self.server_epoch.max(rejected.current_epoch);
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

    /// The Pi accepted the own commit: merge it and finish its job.
    async fn merge_own(&mut self) -> Result<(), EngineError> {
        let Some(inflight) = self.inflight.take() else {
            return Ok(());
        };
        let now = self.now();
        let store = Arc::clone(&self.shared.store);
        let merged = match self.group.as_mut() {
            Some(group) => store.transaction(|| -> Result<(), EngineError> {
                let had_path = group
                    .pending_commit()
                    .is_some_and(|c| c.update_path_leaf_node().is_some());
                group
                    .merge_pending_commit(store.mls())
                    .map_err(|e| MlsError::new("merging the own commit", e))?;
                store.clear_outgoing_commit()?;
                if had_path {
                    store.set_own_leaf_updated(now)?;
                }
                Ok(())
            }),
            None => Ok(()),
        };
        if let Err(error) = merged {
            self.reload_group()?;
            inflight.job.fail(ClientError::from(error));
            return Ok(());
        }
        if let Some(group) = self.group.as_ref() {
            tracing::info!(epoch = group.epoch().as_u64(), "own commit merged");
        }
        self.emit(Event::MembersChanged);
        self.finish(inflight.job).await;
        Ok(())
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
