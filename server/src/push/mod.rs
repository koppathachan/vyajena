//! Wake pings (pi-runtime: Push provider interface; message-delivery:
//! Content-free wake pings; pi-server-core task 9.1).
//!
//! The messaging service asks for a ping when it adds an entry for a device
//! that has no open `Connect` stream and has a push token, at most once per
//! device per [`WAKE_PING_MIN_INTERVAL`]. A ping carries nothing but the
//! token: no content, sender, group id or count. Real FCM and APNs providers
//! come with the Flutter change; until then [`LoggingProvider`] only logs.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::SystemTime;

use protocol::device_identity::DeviceId;
pub use protocol::message_delivery::WAKE_PING_MIN_INTERVAL;

use crate::pb;
use crate::runtime::server_keys::hex;

/// Sends one content-free wake ping to a device's push token.
#[tonic::async_trait]
pub trait PushProvider: Send + Sync + 'static {
    /// Wakes `device_id` through `token`. Failures are the provider's to log;
    /// a lost ping only delays delivery until the app next connects.
    async fn wake(&self, device_id: DeviceId, token: pb::PushToken);
}

/// The provider used when no real one is configured: logs the recipient
/// device id and platform, and sends nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct LoggingProvider;

#[tonic::async_trait]
impl PushProvider for LoggingProvider {
    async fn wake(&self, device_id: DeviceId, token: pb::PushToken) {
        let platform = pb::PushPlatform::try_from(token.platform)
            .map(|p| p.as_str_name())
            .unwrap_or("PUSH_PLATFORM_UNKNOWN");
        tracing::info!(
            device_id = %hex(&device_id),
            platform,
            "wake ping (logging push provider: nothing sent)"
        );
    }
}

/// When each device was last pinged, so none gets more than one ping per
/// [`WAKE_PING_MIN_INTERVAL`].
#[derive(Debug, Default)]
pub struct PingLimiter {
    last: Mutex<HashMap<DeviceId, SystemTime>>,
}

impl PingLimiter {
    /// Whether `device_id` may be pinged at `now`; if so, records the ping.
    /// A ping inside the interval is dropped, not deferred: the earlier ping
    /// already woke the app, which fetches everything when it connects.
    pub fn take(&self, device_id: &DeviceId, now: SystemTime) -> bool {
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        let allowed = last.get(device_id).is_none_or(|at| {
            now.duration_since(*at)
                .is_ok_and(|since| since >= WAKE_PING_MIN_INTERVAL)
        });
        if allowed {
            last.insert(*device_id, now);
        }
        allowed
    }
}
