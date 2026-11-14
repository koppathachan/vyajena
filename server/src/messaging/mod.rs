//! Messaging service: the `Connect` stream, sequencing, queues and the commit
//! log (pi-server-core tasks 7.1-7.4), the Pi's inactivity Removes and re-add
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
pub use stream::IDLE_CHECK_INTERVAL;
pub use sweep::{SWEEP_INTERVAL, sweep};

use crate::auth::caller;
use crate::pb;
use crate::pb::messaging_server::{Messaging, MessagingServer};
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
/// per minute (message-delivery: Content-free wake pings).
pub(crate) fn notify(state: &State, devices: Vec<DeviceId>) {
    if devices.is_empty() {
        return;
    }
    let offline = state.hub.wake(&devices);
    if offline.is_empty() {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        let wanted = offline.clone();
        let tokens = state
            .store
            .call(move |conn| {
                let mut tokens = Vec::new();
                for device_id in wanted {
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
            if state.hub.pings().take(&device_id, state.clock.now()) {
                state.push.wake(device_id, token).await;
            }
        }
    });
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
        let mut every = tokio::time::interval(SWEEP_INTERVAL);
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
