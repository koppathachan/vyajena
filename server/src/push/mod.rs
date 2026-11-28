//! Wake pings (server-runtime: Push provider interface, Push configuration, Push
//! errors, Egress proxy; message-delivery: Content-free wake pings).
//!
//! The messaging service asks for a ping when it adds an entry for a device
//! that has no open `Connect` stream and has a push token, at most once per
//! device per [`WAKE_PING_MIN_INTERVAL`]. A ping carries the token and the one
//! fixed text [`WAKE_TEXT`]: no content, sender, group id or count. Both
//! providers build it in [`payload`] (smarter-notifications: the wake payload
//! is content-free and metadata-minimal).
//!
//! - [`FcmProvider`]: FCM HTTP v1 with a Google service account (push design D2).
//! - [`ApnsProvider`]: APNs over HTTP/2 with a `.p8` key (design D3).
//! - [`LoggingProvider`]: logs the device id and platform, sends nothing.
//! - [`Providers`]: picks one of those by the token's platform.
//! - [`proxy`]: the CONNECT proxy the server's providers dial out through (D5).

mod apns;
mod config;
mod fcm;
mod http;
mod jwt;
pub mod payload;
pub mod proxy;

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use protocol::device_identity::DeviceId;
pub use protocol::message_delivery::WAKE_PING_MIN_INTERVAL;
use rustls_pki_types::CertificateDer;

pub use apns::ApnsProvider;
pub use config::{
    APNS_KEY_PREFIX, ApnsKey, Credentials, FCM_SERVICE_ACCOUNT_FILE, FcmServiceAccount,
    PushConfigError, PushSettings,
};
pub use fcm::FcmProvider;

use crate::pb;

/// The only text a wake ping shows, on every platform, for every message
/// (message-delivery: Content-free wake pings; owner, 2026-11-16).
pub const WAKE_TEXT: &str = "New message";

/// Collapse key / tag / `apns-collapse-id`: a new ping replaces the shown one.
pub const COLLAPSE_KEY: &str = "wake";

/// What became of one wake ping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeOutcome {
    /// The platform accepted it (or the logging provider logged it).
    Sent,
    /// The platform says the token is no longer valid: the server clears it
    /// (server-runtime: Push errors).
    TokenInvalid,
    /// Not sent (overload after the retries, an error, no credentials); the
    /// device catches up when the app next connects.
    Dropped,
}

impl WakeOutcome {
    /// For logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sent => "sent",
            Self::TokenInvalid => "token_invalid",
            Self::Dropped => "dropped",
        }
    }
}

/// Sends one content-free wake ping to a device's push token.
#[tonic::async_trait]
pub trait PushProvider: Send + Sync + 'static {
    /// Wakes `device_id` through `token`. A lost ping only delays delivery
    /// until the app next connects.
    async fn wake(&self, device_id: DeviceId, token: pb::PushToken) -> WakeOutcome;
}

/// The token's platform name for logs.
pub fn platform_name(token: &pb::PushToken) -> &'static str {
    pb::PushPlatform::try_from(token.platform)
        .map(|p| p.as_str_name())
        .unwrap_or("PUSH_PLATFORM_UNKNOWN")
}

/// The provider used when no real one is configured: logs the recipient
/// device id and platform, and sends nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct LoggingProvider;

#[tonic::async_trait]
impl PushProvider for LoggingProvider {
    async fn wake(&self, device_id: DeviceId, token: pb::PushToken) -> WakeOutcome {
        tracing::info!(
            device = %crate::runtime::redact(&device_id),
            platform = platform_name(&token),
            "wake ping (logging push provider: nothing sent)"
        );
        WakeOutcome::Sent
    }
}

/// Where the providers send, and how they retry. [`Options::default`] is the
/// real thing; tests point it at fake servers.
#[derive(Debug, Clone)]
pub struct Options {
    /// FCM's base URL (`https://fcm.googleapis.com`).
    pub fcm_base: String,
    /// APNs production (`https://api.push.apple.com`).
    pub apns_production: String,
    /// APNs sandbox (`https://api.sandbox.push.apple.com`), for Xcode builds.
    pub apns_sandbox: String,
    /// The CONNECT proxy every request goes through (`http://host:port`).
    pub proxy: Option<String>,
    /// Extra trusted roots on top of the webpki roots (tests only).
    pub extra_roots: Vec<CertificateDer<'static>>,
    /// The first backoff on 429/5xx; it doubles per retry.
    pub backoff_base: Duration,
    /// Retries after the first attempt.
    pub max_retries: u32,
    /// A backoff longer than this drops the ping at once (the gate still waits).
    pub max_wait: Duration,
    /// Per HTTP request.
    pub request_timeout: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            fcm_base: "https://fcm.googleapis.com".into(),
            apns_production: "https://api.push.apple.com".into(),
            apns_sandbox: "https://api.sandbox.push.apple.com".into(),
            proxy: None,
            extra_roots: Vec::new(),
            backoff_base: Duration::from_secs(1),
            max_retries: 3,
            max_wait: Duration::from_secs(60),
            request_timeout: Duration::from_secs(10),
        }
    }
}

/// The real providers, one per platform that has credentials; tokens of a
/// platform without one go to [`LoggingProvider`] (server-runtime: Push provider
/// interface, "Only APNs configured").
pub struct Providers {
    fcm: Option<FcmProvider>,
    apns: Option<ApnsProvider>,
}

impl Providers {
    /// Builds the providers for `credentials`.
    pub fn new(
        credentials: Credentials,
        settings: &PushSettings,
        options: &Options,
    ) -> Result<Self, PushConfigError> {
        let mut options = options.clone();
        if settings.proxy.is_some() {
            options.proxy = settings.proxy.clone();
        }
        let fcm = match credentials.fcm {
            Some(account) => Some(FcmProvider::new(account, &options)?),
            None => None,
        };
        let apns = match credentials.apns {
            Some(key) => Some(ApnsProvider::new(
                key,
                settings.apns_team_id.clone(),
                settings.apns_topic.clone(),
                &options,
            )?),
            None => None,
        };
        Ok(Self { fcm, apns })
    }

    /// Reads `push.toml` and its credentials folder and builds the providers
    /// (the server's `--push`).
    pub fn load(path: &std::path::Path, options: &Options) -> Result<Self, PushConfigError> {
        let settings = PushSettings::load(path)?;
        let credentials = Credentials::load(&settings.credentials_dir)?;
        Self::new(credentials, &settings, options)
    }

    /// Whether FCM pings are really sent.
    pub fn has_fcm(&self) -> bool {
        self.fcm.is_some()
    }

    /// Whether APNs pings are really sent.
    pub fn has_apns(&self) -> bool {
        self.apns.is_some()
    }
}

#[tonic::async_trait]
impl PushProvider for Providers {
    async fn wake(&self, device_id: DeviceId, token: pb::PushToken) -> WakeOutcome {
        let platform = pb::PushPlatform::try_from(token.platform).ok();
        let outcome = match (platform, &self.fcm, &self.apns) {
            (Some(pb::PushPlatform::Fcm), Some(fcm), _) => fcm.send(&token.token).await,
            (Some(pb::PushPlatform::Apns), _, Some(apns)) => apns.send(&token.token).await,
            _ => return LoggingProvider.wake(device_id, token).await,
        };
        let device = crate::runtime::redact(&device_id);
        let platform = platform_name(&token);
        match outcome {
            WakeOutcome::Dropped => {
                tracing::warn!(%device, platform, outcome = outcome.as_str(), "wake ping")
            }
            _ => tracing::info!(%device, platform, outcome = outcome.as_str(), "wake ping"),
        }
        outcome
    }
}

/// What kind of wake ping is being rate-limited (message-delivery: Content-free
/// wake pings). The class decides which interval applies, so a real message is
/// never starved by an unrelated nudge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PingClass {
    /// A newly-queued user-visible message — the write path, or the stream-loss
    /// re-ping that exists to deliver a pending message. Floored by the short
    /// message interval and never suppressed by a nudge.
    Message,
    /// A catch-up sweep ping or an own-access admin-event ping. Keeps the 60 s
    /// cadence and does not reset the message interval.
    Nudge,
}

/// The last ping times for one device.
#[derive(Debug, Clone, Copy)]
struct Last {
    /// The last ping of any class.
    any: SystemTime,
    /// The last `Message`-class ping, if any.
    message: Option<SystemTime>,
}

/// When each device was last pinged, per class, so a real message is never starved
/// by a catch-up or own-access nudge (message-delivery: Content-free wake pings).
#[derive(Debug, Default)]
pub struct PingLimiter {
    last: Mutex<HashMap<DeviceId, Last>>,
}

impl PingLimiter {
    /// Whether `device_id` may be pinged at `now` for `class`; if so, records it.
    /// A `Message` is allowed once `message_interval` has passed since the last
    /// message ping, ignoring any nudge in between; a `Nudge` is allowed once
    /// `nudge_interval` has passed since the last ping of any class and does not
    /// reset the message interval. A ping inside its interval is dropped, not
    /// deferred: the device catches up on its next connect, and the on-device
    /// collapse key means repeats collapse to one notification.
    pub fn take(
        &self,
        device_id: &DeviceId,
        now: SystemTime,
        class: PingClass,
        message_interval: Duration,
        nudge_interval: Duration,
    ) -> bool {
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = last.get(device_id).copied();
        let elapsed = |at: SystemTime, interval: Duration| {
            now.duration_since(at).is_ok_and(|since| since >= interval)
        };
        let allowed = match class {
            PingClass::Message => entry
                .and_then(|l| l.message)
                .is_none_or(|at| elapsed(at, message_interval)),
            PingClass::Nudge => entry.is_none_or(|l| elapsed(l.any, nudge_interval)),
        };
        if allowed {
            let message = match class {
                PingClass::Message => Some(now),
                PingClass::Nudge => entry.and_then(|l| l.message),
            };
            last.insert(*device_id, Last { any: now, message });
        }
        allowed
    }
}
