//! One `Connect` stream (wire-transport: Single persistent messaging stream;
//! Heartbeats; Protocol version negotiation).
//!
//! The stream task waits for `Hello`, registers with the [`Hub`] (ending an
//! older stream of the device with `ABORTED`), sends the connect-time notices
//! and the catch-up, then serves client frames and hub events until the
//! client leaves, the stream idles out or the hub closes it. Everything the
//! device receives comes from its stored deliveries, read in `server_seq`
//! order after the last one this stream sent, so catch-up and live traffic
//! are one ordered sequence (message-delivery: Ordered delivery and
//! catch-up).
//!
//! [`Hub`]: super::Hub

use std::time::Duration;

use protocol::device_identity::DeviceId;
use protocol::key_directory::KEY_PACKAGES_LOW_WATER_MARK;
use protocol::wire_transport::{STREAM_IDLE_TIMEOUT, SUPPORTED_PROTOCOL_VERSIONS};
use tokio::sync::{mpsc, watch};
use tonic::{Status, Streaming};

use super::hub::Event;
use super::log;
use super::write::{self, Context, Outcome};
use crate::auth::Caller;
use crate::dms;
use crate::membership::{group, inactivity, joins};
use crate::pb;
use crate::pb::client_frame::Frame as In;
use crate::pb::envelope::Kind;
use crate::pb::server_frame::Frame as Out;
use crate::runtime::State;
use crate::runtime::clock::to_timestamp;
use crate::store::{StoreError, records};

/// How often a stream checks the clock for the idle timeout. The deadline
/// itself is read from the injectable clock (design D7), so tests can move
/// past it without waiting 90 seconds.
pub const IDLE_CHECK_INTERVAL: Duration = Duration::from_secs(1);

/// Entries read from the database per batch.
const DELIVERY_BATCH: usize = 128;

pub(crate) type Outgoing = mpsc::Sender<Result<pb::ServerFrame, Status>>;

fn frame(frame: Out) -> pb::ServerFrame {
    pb::ServerFrame { frame: Some(frame) }
}

/// The stream ended; the reason was already sent or the client is gone.
struct Ended;

struct Session {
    state: State,
    caller: Caller,
    out: Outgoing,
    /// The highest `server_seq` sent on this stream.
    sent_up_to: u64,
}

impl Session {
    async fn send(&self, frame: pb::ServerFrame) -> Result<(), Ended> {
        self.out.send(Ok(frame)).await.map_err(|_| Ended)
    }

    async fn fail(&self, status: Status) -> Ended {
        let _ = self.out.send(Err(status)).await;
        Ended
    }

    async fn store<R, F>(&self, job: F) -> Result<R, Ended>
    where
        R: Send + 'static,
        F: FnOnce(&mut rusqlite::Connection) -> Result<R, StoreError> + Send + 'static,
    {
        match self.state.store.call(job).await {
            Ok(value) => Ok(value),
            Err(error) => Err(self.fail(Status::from(error)).await),
        }
    }

    /// Sends every stored entry after `sent_up_to`, in order.
    async fn deliver_pending(&mut self) -> Result<(), Ended> {
        self.deliver_below(u64::MAX).await
    }

    /// Sends every stored entry after `sent_up_to` and below `below`, in
    /// order.
    async fn deliver_below(&mut self, below: u64) -> Result<(), Ended> {
        loop {
            let device_id = self.caller.device_id;
            let after = self.sent_up_to;
            let server_id = self.state.config.server_id.clone();
            let batch = self
                .store(move |conn| {
                    log::pending(conn, &server_id, &device_id, after, below, DELIVERY_BATCH)
                })
                .await?;
            let full = batch.len() == DELIVERY_BATCH;
            for (seq, envelope) in batch {
                self.send(frame(Out::Envelope(envelope))).await?;
                self.sent_up_to = seq;
            }
            if !full {
                return Ok(());
            }
        }
    }

    /// Connect-time work: presence, the cursor, and the notices this device
    /// is owed; also tells the devices that can re-add it that it is back.
    async fn on_connect(&mut self) -> Result<(), Ended> {
        let caller = self.caller.clone();
        let admin = self.state.config.admin_username.clone();
        let now = self.state.clock.now();
        let server_id = self.state.config.server_id.clone();
        let info = self
            .store(move |conn| {
                let tx = conn.transaction()?;
                let info = connect_info(&tx, &caller, &admin, &server_id, now)?;
                tx.commit()?;
                Ok(info)
            })
            .await?;
        self.sent_up_to = info.cursor;
        for frame in info.frames {
            self.send(frame).await?;
        }
        if !info.gaps_sent.is_empty() {
            let device_id = self.caller.device_id;
            let groups = info.gaps_sent;
            self.store(move |conn| {
                for group_id in &groups {
                    log::clear_gap(conn, &device_id, group_id)?;
                }
                Ok(())
            })
            .await?;
        }
        for (target, notice) in info.rejoin {
            self.state.hub.send_frame(&target, notice);
        }
        Ok(())
    }

    async fn on_frame(&mut self, frame: pb::ClientFrame) -> Result<(), Ended> {
        match frame.frame {
            Some(In::Heartbeat(_)) => {
                self.send(self::frame(Out::Heartbeat(pb::Heartbeat {})))
                    .await
            }
            Some(In::Hello(_)) => Err(self
                .fail(Status::invalid_argument("Hello is only the first frame"))
                .await),
            Some(In::Ack(ack)) => self.on_ack(ack).await,
            Some(In::Envelope(envelope)) => self.on_envelope(envelope).await,
            None => Ok(()),
        }
    }

    /// message-delivery: Acknowledgement. Cumulative, and never beyond what
    /// this stream sent, so a client cannot drop entries it was never given.
    async fn on_ack(&mut self, ack: pb::Ack) -> Result<(), Ended> {
        let device_id = self.caller.device_id;
        let upto = ack.server_seq.min(self.sent_up_to);
        self.store(move |conn| {
            let tx = conn.transaction()?;
            // direct-messages: DM closed on user removal. `server_seq` 0
            // acknowledges the `DmClosed` of that group.
            if ack.server_seq == 0 {
                dms::ack_closed(&tx, &device_id, &ack.group_id)?;
                tx.commit()?;
                return Ok(());
            }
            if group::row(&tx, &ack.group_id)?.is_some() {
                log::ack(&tx, &device_id, upto)?;
                tx.commit()?;
            }
            Ok(())
        })
        .await
    }

    async fn on_envelope(&mut self, envelope: pb::Envelope) -> Result<(), Ended> {
        let msg_id = envelope.msg_id.clone();
        let precheck = write::precheck(&envelope);
        let is_commit = matches!(precheck, Ok(Kind::Commit));
        let outcome = match precheck {
            Err(reason) => Outcome::Rejected {
                reason,
                current_epoch: 0,
            },
            Ok(kind) => {
                let caller = self.caller.clone();
                let keys = self.state.keys.clone();
                let server_id = self.state.config.server_id.clone();
                let now = self.state.clock.now();
                self.store(move |conn| {
                    let tx = conn.transaction()?;
                    let ctx = Context {
                        server_id: &server_id,
                        keys: &keys,
                        now,
                    };
                    let outcome = write::submit(&tx, &ctx, &caller, &envelope, kind)?;
                    if matches!(outcome, Outcome::Accepted { .. }) {
                        tx.commit()?;
                    }
                    Ok(outcome)
                })
                .await?
            }
        };
        let result = match outcome {
            Outcome::Accepted {
                server_seq,
                accepted_at,
                notify,
                notices,
            } => {
                tracing::debug!(
                    server_seq,
                    recipients = notify.len(),
                    "accepted an envelope"
                );
                super::notify(&self.state, notify);
                for (device, notice) in notices {
                    self.state.hub.send_frame(&device, notice);
                }
                // message-delivery: Ordered delivery and catch-up. The
                // committer reads every entry sequenced before its commit
                // (in the old epoch) before it learns the commit was
                // accepted and merges it.
                if is_commit {
                    self.deliver_below(server_seq).await?;
                }
                pb::send_result::Result::Accepted(pb::send_result::Accepted {
                    server_seq,
                    accepted_at: Some(to_timestamp(accepted_at)),
                })
            }
            Outcome::Rejected {
                reason,
                current_epoch,
            } => {
                tracing::debug!(reason = reason.as_str_name(), "rejected an envelope");
                pb::send_result::Result::Rejected(pb::send_result::Rejected {
                    reason: reason as i32,
                    current_epoch,
                })
            }
        };
        self.send(frame(Out::SendResult(pb::SendResult {
            msg_id,
            result: Some(result),
        })))
        .await
    }

    fn idle(&self, last_frame: std::time::SystemTime) -> bool {
        self.state
            .clock
            .now()
            .duration_since(last_frame)
            .is_ok_and(|idle| idle >= STREAM_IDLE_TIMEOUT)
    }
}

fn idle_status() -> Status {
    Status::unavailable("no frame received for 90 seconds")
}

/// What a device is owed when it connects.
struct ConnectInfo {
    cursor: u64,
    frames: Vec<pb::ServerFrame>,
    /// The groups whose gap notice `frames` carries.
    gaps_sent: Vec<Vec<u8>>,
    /// Rejoin notices for other devices, about this one.
    rejoin: Vec<(DeviceId, pb::ServerFrame)>,
}

pub(crate) fn device_ref(device_id: &DeviceId, username: &str, server_id: &str) -> pb::DeviceRef {
    pb::DeviceRef {
        user_id: Some(pb::UserId {
            username: username.to_owned(),
            server_id: server_id.to_owned(),
        }),
        device_id: device_id.to_vec(),
    }
}

pub(crate) fn rejoin_notice(group_id: &[u8], device: pb::DeviceRef) -> pb::ServerFrame {
    frame(Out::RejoinNotice(pb::RejoinNotice {
        group_id: group_id.to_vec(),
        device: Some(device),
    }))
}

/// Whether `helper` (of `helper_user`) may re-add a device of `user`: one of
/// the user's own other devices, or an admin device (group-membership:
/// Automatic re-add).
pub(crate) fn can_readd(
    helper: &DeviceId,
    helper_user: &str,
    device: &DeviceId,
    user: &str,
    admin: &str,
) -> bool {
    helper != device && (helper_user == user || helper_user == admin)
}

fn connect_info(
    conn: &rusqlite::Connection,
    caller: &Caller,
    admin: &str,
    server_id: &str,
    now: std::time::SystemTime,
) -> Result<ConnectInfo, StoreError> {
    let device_id = caller.device_id;
    inactivity::touch(conn, &device_id, now)?;
    let cursor = log::cursor(conn, &device_id)?;
    let mut frames = Vec::new();
    let mut rejoin = Vec::new();
    let mut gaps_sent = Vec::new();
    if let Some((group_id, _)) = log::group_row(conn)? {
        if let Some(inactive) = inactivity::get(conn, &device_id)? {
            // group-membership: Automatic re-add. It was removed; the devices
            // that can re-add it hear that it is back. Its DMs follow once
            // the main group has it again (direct-messages: Inactivity across
            // groups), so it hears of its removal from each of them now.
            inactivity::mark_returned(conn, &device_id, now)?;
            let removed = |group_id: &[u8]| {
                frame(Out::RemovedNotice(pb::RemovedNotice {
                    group_id: group_id.to_vec(),
                    missed_since: Some(to_timestamp(inactive.missed_since)),
                }))
            };
            frames.push(removed(&group_id));
            for dm in group::open_dms(conn)? {
                if let group::GroupKind::Dm { users, .. } = &dm.kind
                    && (users.0 == caller.username || users.1 == caller.username)
                {
                    frames.push(removed(&dm.group_id));
                }
            }
            let me = device_ref(&device_id, &caller.username, server_id);
            for (helper, helper_user) in inactivity::active_members(conn, &group_id)? {
                if can_readd(&helper, &helper_user, &device_id, &caller.username, admin) {
                    rejoin.push((helper, rejoin_notice(&group_id, me.clone())));
                }
            }
        } else if inactivity::active_members(conn, &group_id)?
            .iter()
            .any(|(id, _)| *id == device_id)
        {
            // group-membership: Automatic add of new users. An admin member
            // device hears about every device still waiting for its first
            // add.
            if caller.username == admin {
                for (waiting, user) in joins::waiting(conn)? {
                    frames.push(joins::notice(&group_id, &waiting, &user, server_id));
                }
            }
            // This device can re-add devices that came back while nobody
            // able to re-add them was online.
            for (waiting, user) in inactivity::waiting(conn)? {
                if can_readd(&device_id, &caller.username, &waiting, &user, admin) {
                    frames.push(rejoin_notice(
                        &group_id,
                        device_ref(&waiting, &user, server_id),
                    ));
                }
            }
            // direct-messages: DM members kept complete. The devices its DMs
            // still need.
            frames.extend(joins::dm_notices_for(conn, &device_id, server_id)?);
        }
        for (group_id, at) in log::gaps(conn, &device_id)? {
            frames.push(frame(Out::GapNotice(pb::GapNotice {
                group_id: group_id.clone(),
                newest_expired_at: Some(to_timestamp(at)),
            })));
            gaps_sent.push(group_id);
        }
        // direct-messages: DM closed on user removal, until acknowledged.
        frames.extend(dms::closed_notices(conn, &device_id)?);
    }
    // key-directory: Replenishment signal, for a count that dropped while
    // the device was away.
    let remaining = records::one_time_key_packages(conn, &device_id)?;
    if usize::try_from(remaining).unwrap_or(usize::MAX) < KEY_PACKAGES_LOW_WATER_MARK {
        frames.push(frame(Out::LowKeyPackages(pb::LowKeyPackages { remaining })));
    }
    Ok(ConnectInfo {
        cursor,
        frames,
        gaps_sent,
        rejoin,
    })
}

/// Runs one `Connect` stream until it ends.
pub(crate) async fn run(
    state: State,
    caller: Caller,
    mut inbound: Streaming<pb::ClientFrame>,
    out: Outgoing,
) {
    let opened_at = std::time::Instant::now();
    let mut session = Session {
        state,
        caller,
        out,
        sent_up_to: 0,
    };
    let mut tick = tokio::time::interval(IDLE_CHECK_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_frame = session.state.clock.now();
    let mut stopping = session.state.hub.stopping();

    // wire-transport: Protocol version negotiation.
    let hello = loop {
        tokio::select! {
            message = inbound.message() => break message,
            _ = tick.tick() => {
                if session.idle(last_frame) {
                    session.fail(idle_status()).await;
                    return;
                }
            }
            () = session.out.closed() => return,
            () = stopped(&mut stopping) => {
                session.fail(stopping_status()).await;
                return;
            }
        }
    };
    match hello {
        Ok(Some(pb::ClientFrame {
            frame: Some(In::Hello(hello)),
        })) if SUPPORTED_PROTOCOL_VERSIONS.contains(&hello.protocol_version) => {}
        Ok(Some(pb::ClientFrame {
            frame: Some(In::Hello(hello)),
        })) => {
            session
                .fail(Status::failed_precondition(format!(
                    "protocol version {} is not supported; supported versions: {:?}",
                    hello.protocol_version, SUPPORTED_PROTOCOL_VERSIONS
                )))
                .await;
            return;
        }
        Ok(Some(_)) => {
            session
                .fail(Status::invalid_argument("the first frame must be Hello"))
                .await;
            return;
        }
        Ok(None) | Err(_) => return,
    }
    last_frame = session.state.clock.now();

    let device_id = session.caller.device_id;
    let (stream_id, mut events, reopens) = session
        .state
        .hub
        .register(device_id, &session.caller.username);
    // relay-privacy-logging: Connection metrics — one event per stream open,
    // the device id redacted.
    tracing::info!(
        device = %crate::runtime::redact(&device_id),
        open_ms = opened_at.elapsed().as_millis() as u64,
        reopens,
        active_streams = session.state.hub.active(),
        "stream open"
    );
    let _ = serve(
        &mut session,
        &mut inbound,
        &mut events,
        &mut tick,
        &mut stopping,
        last_frame,
    )
    .await;
    session.state.hub.unregister(&device_id, stream_id);
    // The device was connected until now (group-membership: Inactive devices
    // are kept out).
    let now = session.state.clock.now();
    let _ = session
        .state
        .store
        .call(move |conn| inactivity::touch(conn, &device_id, now))
        .await;
    tracing::debug!(device = %crate::runtime::redact(&device_id), "stream closed");
}

async fn serve(
    session: &mut Session,
    inbound: &mut Streaming<pb::ClientFrame>,
    events: &mut mpsc::UnboundedReceiver<Event>,
    tick: &mut tokio::time::Interval,
    stopping: &mut watch::Receiver<bool>,
    mut last_frame: std::time::SystemTime,
) -> Result<(), Ended> {
    session.on_connect().await?;
    session.deliver_pending().await?;
    loop {
        tokio::select! {
            message = inbound.message() => match message {
                Ok(Some(frame)) => {
                    last_frame = session.state.clock.now();
                    session.on_frame(frame).await?;
                }
                Ok(None) | Err(_) => return Ok(()),
            },
            event = events.recv() => match event {
                Some(Event::Wake) => session.deliver_pending().await?,
                Some(Event::Frame(frame)) => session.send(*frame).await?,
                Some(Event::Close(status)) => return Err(session.fail(status).await),
                None => return Ok(()),
            },
            _ = tick.tick() => {
                if session.idle(last_frame) {
                    return Err(session.fail(idle_status()).await);
                }
            }
            () = session.out.closed() => return Ok(()),
            // Polled only between frames: a frame in progress (an envelope
            // being accepted) finishes first (server-runtime: Graceful stop).
            () = stopped(stopping) => return Err(session.fail(stopping_status()).await),
        }
    }
}

/// Completes once the server is stopping.
async fn stopped(stopping: &mut watch::Receiver<bool>) {
    // An error means the hub is gone, which also means stopping.
    let _ = stopping.wait_for(|stop| *stop).await;
}

fn stopping_status() -> Status {
    Status::unavailable("the server is stopping; reconnect later")
}
