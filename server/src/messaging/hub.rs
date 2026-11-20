//! The registry of open `Connect` streams: at most one per device
//! (wire-transport: Single persistent messaging stream). Other parts of the
//! Server reach a connected device only through it: to wake its stream when new
//! entries are stored, to hand it a notice, or to close it (an older stream
//! of the same device, a revoked device).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use protocol::device_identity::DeviceId;
use tokio::sync::{mpsc, watch};
use tonic::Status;

use crate::pb;
use crate::push::PingLimiter;

/// What a stream task is told.
#[derive(Debug)]
pub(crate) enum Event {
    /// New entries were stored for the device: send what it has not had yet.
    Wake,
    /// Send this frame.
    Frame(Box<pb::ServerFrame>),
    /// End the stream with this status.
    Close(Status),
}

struct Registered {
    id: u64,
    username: String,
    events: mpsc::UnboundedSender<Event>,
}

/// The open streams, keyed by device.
#[derive(Default)]
pub struct Hub {
    streams: Mutex<HashMap<DeviceId, Registered>>,
    next_id: AtomicU64,
    /// How many times each device's stream has opened this process run
    /// (relay-privacy-logging: Connection metrics). Never cleared, so a
    /// reconnect loop is visible.
    reopens: Mutex<HashMap<DeviceId, u64>>,
    pings: PingLimiter,
    /// Set once when the server stops (server-runtime: Graceful stop).
    stopping: watch::Sender<bool>,
}

impl std::fmt::Debug for Hub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hub")
            .field("connected", &self.connected().len())
            .finish_non_exhaustive()
    }
}

impl Hub {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    fn streams(&self) -> std::sync::MutexGuard<'_, HashMap<DeviceId, Registered>> {
        self.streams.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Registers a new stream for `device_id`, ending the device's older
    /// stream, if any, with `ABORTED`.
    pub(crate) fn register(
        &self,
        device_id: DeviceId,
        username: &str,
    ) -> (u64, mpsc::UnboundedReceiver<Event>, u64) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (events, receiver) = mpsc::unbounded_channel();
        let older = self.streams().insert(
            device_id,
            Registered {
                id,
                username: username.to_owned(),
                events,
            },
        );
        if let Some(older) = older {
            let _ = older.events.send(Event::Close(Status::aborted(
                "a newer Connect stream opened for this device",
            )));
        }
        let reopens = {
            let mut counts = self.reopens.lock().unwrap_or_else(PoisonError::into_inner);
            let n = counts.entry(device_id).or_insert(0);
            *n += 1;
            *n
        };
        (id, receiver, reopens)
    }

    /// The number of live streams (relay-privacy-logging: Connection metrics).
    pub(crate) fn active(&self) -> usize {
        self.streams().len()
    }

    /// Removes stream `id` of `device_id`, unless a newer one replaced it.
    pub(crate) fn unregister(&self, device_id: &DeviceId, id: u64) {
        let mut streams = self.streams();
        if streams.get(device_id).is_some_and(|r| r.id == id) {
            streams.remove(device_id);
        }
    }

    /// Whether `device_id` has an open stream.
    pub fn is_connected(&self, device_id: &DeviceId) -> bool {
        self.streams().contains_key(device_id)
    }

    /// Every device with an open stream, with its username.
    pub fn connected(&self) -> Vec<(DeviceId, String)> {
        self.streams()
            .iter()
            .map(|(id, r)| (*id, r.username.clone()))
            .collect()
    }

    fn send(&self, device_id: &DeviceId, event: Event) -> bool {
        self.streams()
            .get(device_id)
            .is_some_and(|r| r.events.send(event).is_ok())
    }

    /// Hands `frame` to `device_id`'s stream; false if it has none.
    pub(crate) fn send_frame(&self, device_id: &DeviceId, frame: pb::ServerFrame) -> bool {
        self.send(device_id, Event::Frame(Box::new(frame)))
    }

    /// Ends `device_id`'s stream, if open, with `status`.
    pub(crate) fn close(&self, device_id: &DeviceId, status: Status) {
        self.send(device_id, Event::Close(status));
    }

    /// Wakes the streams of the connected devices among `devices` and returns
    /// the others (candidates for a wake ping).
    pub(crate) fn wake(&self, devices: &[DeviceId]) -> Vec<DeviceId> {
        let mut offline = Vec::new();
        for device_id in devices {
            if !self.send(device_id, Event::Wake) && !offline.contains(device_id) {
                offline.push(*device_id);
            }
        }
        offline
    }

    /// Ends every open stream, and every stream that opens from now on, with
    /// `UNAVAILABLE` once its current frame is handled, so a stop never waits
    /// on a device's open stream (server-runtime: Graceful stop).
    pub fn stop(&self) {
        self.stopping.send_replace(true);
    }

    /// Watches for [`Hub::stop`].
    pub(crate) fn stopping(&self) -> watch::Receiver<bool> {
        self.stopping.subscribe()
    }

    /// The wake ping rate limiter.
    pub(crate) fn pings(&self) -> &PingLimiter {
        &self.pings
    }
}
