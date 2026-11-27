//! Messaging service: the `Connect` stream, sequencing, queues and the commit
//! log (server-core tasks 7.1-7.4), the server's inactivity Removes and re-add
//! notices (8.1, 8.2), and when to send wake pings (9.1).
//!
//! - [`hub`]: the open streams, one per device.
//! - `stream`: one stream's life: Hello, notices, catch-up, frames.
//! - `write`: the D4 write path for one envelope.
//! - `log`: the sequence, envelope, delivery, cursor and dedup tables.
//! - [`sweep`]: inactivity, expiry and dedup upkeep driven by the clock.

pub mod hub;
mod log;
mod stream;
pub mod sweep;
mod write;

use protocol::device_identity::DeviceId;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

pub use hub::Hub;
pub(crate) use log::group_row;
pub use stream::IDLE_CHECK_INTERVAL;
pub(crate) use stream::{can_readd, device_ref, rejoin_notice};
pub use sweep::{SWEEP_INTERVAL, sweep};
pub(crate) use write::{Context, mark_inactive};

use crate::auth::caller;
use crate::pb;
use crate::pb::messaging_server::{Messaging, MessagingServer};
use crate::push::WakeOutcome;
use crate::runtime::State;
use crate::store::records;

/// Frames buffered towards one client before the stream waits for it.
const OUTGOING_BUFFER: usize = 64;

/// The gRPC service.
pub fn service(state: &State) -> MessagingServer<MessagingService> {
    MessagingServer::new(MessagingService {
        state: state.clone(),
    })
}

/// Implements `vyajena.v1.Messaging`.
pub struct MessagingService {
    state: State,
}

#[tonic::async_trait]
impl Messaging for MessagingService {
    type ConnectStream = ReceiverStream<Result<pb::ServerFrame, Status>>;

    async fn connect(
        &self,
        request: Request<Streaming<pb::ClientFrame>>,
    ) -> Result<Response<Self::ConnectStream>, Status> {
        let caller = caller(&request)?.clone();
        let inbound = request.into_inner();
        let (out, receiver) = mpsc::channel(OUTGOING_BUFFER);
        tokio::spawn(stream::run(self.state.clone(), caller, inbound, out));
        Ok(Response::new(ReceiverStream::new(receiver)))
    }
}

/// New entries were stored for `devices`: wakes their open streams and sends
/// a wake ping to the others that have a push token, at most once per device
/// per minute (message-delivery: Content-free wake pings). The wake ping is
/// kind-aware (push-kind-aware: Only an application backlog alerts an offline
/// device): an offline device is pinged only when it has a pending
/// `APPLICATION` entry; control envelopes (commit/welcome/proposal) still wake
/// an open stream here but fire no push.
pub(crate) fn notify(state: &State, devices: Vec<DeviceId>) {
    notify_in(state, devices, None);
}

/// Like [`notify`], but for entries that belong to one conversation. The wake
/// ping is suppressed for a device whose user muted `group_id` or set
/// `muted_all` (notification-prefs: A muted conversation gets no wake push); the
/// open-stream wake and the queued entry are unaffected. With `group_id` `None`
/// (a general backlog or catch-up nudge, not one conversation), only `muted_all`
/// suppresses the ping.
pub(crate) fn notify_in(state: &State, devices: Vec<DeviceId>, group_id: Option<Vec<u8>>) {
    if devices.is_empty() {
        return;
    }
    let offline = state.hub.wake(&devices);
    if offline.is_empty() {
        return;
    }
    ping_offline(state, offline, Why::Backlog(group_id));
}

/// An admin-event changed `devices`' own access (message-delivery: Content-free
/// wake for an own-access admin-event): their user was removed from a
/// conversation, a conversation they are in was closed, or they were added to a
/// new named group. The offline ones among them get one content-free wake push
/// although the triggering entry is control traffic, so it needs no
/// `APPLICATION` backlog (it is an enumerated exception to `push-kind-aware`).
/// The payload says nothing of the event. A conversation's mute is ignored, as
/// the event is about the user's access, not its chat; whole-app mute still
/// silences it. Connected devices hear of it on their stream.
pub(crate) fn notify_own_access(state: &State, devices: Vec<DeviceId>) {
    let offline: Vec<DeviceId> = devices
        .into_iter()
        .filter(|device| !state.hub.is_connected(device))
        .collect();
    if offline.is_empty() {
        return;
    }
    ping_offline(state, offline, Why::OwnAccess);
}

/// Why an offline device may be pinged.
enum Why {
    /// New entries for a conversation (or a general nudge, `None`): needs an
    /// `APPLICATION` backlog and honors that conversation's mute.
    Backlog(Option<Vec<u8>>),
    /// An own-access admin-event: no backlog needed, whole-app mute only.
    OwnAccess,
}

/// Sends the content-free wake push to each of `offline` that qualifies under
/// `why` and has a push token, at most once per device per minute.
fn ping_offline(state: &State, offline: Vec<DeviceId>, why: Why) {
    let state = state.clone();
    tokio::spawn(async move {
        let wanted = offline.clone();
        let tokens = state
            .store
            .call(move |conn| {
                let mut tokens = Vec::new();
                for device_id in wanted {
                    // notification-prefs: A muted conversation gets no wake push.
                    // The entry stays queued; only the ping is skipped.
                    let Some(device) = records::device(conn, device_id.as_slice())? else {
                        continue;
                    };
                    // push-kind-aware: Only an application backlog alerts an
                    // offline device. The open streams were already woken above;
                    // a device whose only backlog is control envelopes
                    // (commit/welcome/proposal) has no new message to show, so it
                    // gets no wake push and syncs on its next reconnect. An
                    // own-access admin-event is the enumerated exception.
                    let muted_group = match &why {
                        Why::Backlog(group_id) => {
                            if !log::has_application_backlog(conn, &device_id)? {
                                continue;
                            }
                            group_id.as_deref()
                        }
                        Why::OwnAccess => None,
                    };
                    if crate::prefs::push_muted(conn, &device.username, muted_group)? {
                        continue;
                    }
                    if let Some(token) = records::push_token(conn, &device_id)? {
                        tokens.push((device_id, token));
                    }
                }
                Ok(tokens)
            })
            .await;
        let tokens = match tokens {
            Ok(tokens) => tokens,
            Err(error) => {
                tracing::error!(%error, "reading push tokens failed");
                return;
            }
        };
        for (device_id, token) in tokens {
            // A device that connected meanwhile gets the entries on its stream.
            if state.hub.is_connected(&device_id) {
                continue;
            }
            if state.hub.pings().take(
                &device_id,
                state.clock.now(),
                state.config.limits.wake_ping_min_interval,
            ) {
                let sent = token.token.clone();
                if state.push.wake(device_id, token).await == WakeOutcome::TokenInvalid {
                    clear_invalid_token(&state, device_id, sent).await;
                }
            }
        }
    });
}

/// The platform called `token` invalid (server-runtime: Push errors): clear it,
/// unless the device registered another one meanwhile.
async fn clear_invalid_token(state: &State, device_id: DeviceId, token: String) {
    let cleared = state
        .store
        .call(move |conn| records::clear_push_token(conn, &device_id, &token))
        .await;
    match cleared {
        Ok(true) => tracing::info!(
            device = %crate::runtime::redact(&device_id),
            "cleared a push token the platform called invalid"
        ),
        Ok(false) => {}
        Err(error) => tracing::error!(%error, "clearing a push token failed"),
    }
}

/// Starts the messaging background tasks: closing revoked devices' streams
/// (device-identity: Device revocation), forwarding low-KeyPackage notices
/// (key-directory: Replenishment signal), and the periodic [`sweep`]. Abort
/// the handles when the server stops.
pub fn background(state: &State) -> Vec<JoinHandle<()>> {
    let mut tasks = Vec::new();

    let mut revoked = state.revocations.subscribe();
    let revoker = state.clone();
    tasks.push(tokio::spawn(async move {
        let closed = || Status::permission_denied("this device was revoked");
        loop {
            match revoked.recv().await {
                Ok(device_id) => revoker.hub.close(&device_id, closed()),
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    // Some ids were missed: check every open stream instead.
                    tracing::warn!(missed, "revocation notices lagged");
                    for (device_id, _) in revoker.hub.connected() {
                        let gone = revoker
                            .store
                            .call(move |conn| {
                                Ok(records::device(conn, &device_id)?.is_none_or(|d| d.revoked))
                            })
                            .await
                            .unwrap_or(false);
                        if gone {
                            revoker.hub.close(&device_id, closed());
                        }
                    }
                }
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    }));

    let mut low = state.key_packages_low.subscribe();
    let hub = state.hub.clone();
    tasks.push(tokio::spawn(async move {
        loop {
            match low.recv().await {
                Ok(count) => {
                    hub.send_frame(
                        &count.device_id,
                        pb::ServerFrame {
                            frame: Some(pb::server_frame::Frame::LowKeyPackages(
                                pb::LowKeyPackages {
                                    remaining: count.remaining,
                                },
                            )),
                        },
                    );
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    }));

    let sweeper = state.clone();
    tasks.push(tokio::spawn(async move {
        let mut every = tokio::time::interval(sweeper.config.limits.sweep_interval);
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            every.tick().await;
            if let Err(error) = sweep(&sweeper).await {
                tracing::error!(%error, "sweep failed");
            }
        }
    }));
    tasks
}
