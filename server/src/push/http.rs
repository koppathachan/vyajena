//! What both providers share (push design D4, D5): the HTTPS client (rustls,
//! webpki roots, HTTP/2, through the egress proxy) and the backoff gate.

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tokio::time::Instant;

use super::{Options, PushConfigError};

/// The HTTPS client for one provider (`h2_only` for APNs, which speaks only
/// HTTP/2). Never reads proxy settings from the environment: only
/// `options.proxy`.
pub fn client(options: &Options, h2_only: bool) -> Result<reqwest::Client, PushConfigError> {
    let mut builder = reqwest::Client::builder()
        .use_rustls_tls()
        .https_only(true)
        .timeout(options.request_timeout)
        .connect_timeout(options.request_timeout)
        .pool_idle_timeout(Duration::from_secs(90))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("vyajena-pi");
    if h2_only {
        builder = builder.http2_prior_knowledge();
    }
    for root in &options.extra_roots {
        let cert = reqwest::Certificate::from_der(root.as_ref())
            .map_err(|e| PushConfigError::Client(e.to_string()))?;
        builder = builder.add_root_certificate(cert);
    }
    builder = match &options.proxy {
        Some(proxy) => builder.proxy(
            reqwest::Proxy::https(proxy).map_err(|e| PushConfigError::Client(e.to_string()))?,
        ),
        None => builder.no_proxy(),
    };
    builder
        .build()
        .map_err(|e| PushConfigError::Client(e.to_string()))
}

/// One per provider: after a 429 or 5xx nothing is sent to that platform
/// until the wait is over (pi-runtime: Push errors).
#[derive(Debug, Default)]
pub struct Gate {
    not_before: Mutex<Option<Instant>>,
}

impl Gate {
    /// Waits until the platform may be called again.
    pub async fn wait(&self) {
        let until = *self
            .not_before
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(until) = until {
            tokio::time::sleep_until(until).await;
        }
    }

    /// Holds every send for `delay` from now (never shortens a longer hold).
    pub fn hold(&self, delay: Duration) {
        let until = Instant::now() + delay;
        let mut not_before = self
            .not_before
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if not_before.is_none_or(|t| t < until) {
            *not_before = Some(until);
        }
    }
}

/// The wait before retry number `retry` (0-based): `Retry-After` seconds if
/// the answer had them, else `base`, doubling per retry.
pub fn backoff(base: Duration, retry: u32, retry_after: Option<Duration>) -> Duration {
    retry_after.unwrap_or_else(|| base.saturating_mul(2u32.saturating_pow(retry)))
}

/// `Retry-After` in seconds (the HTTP-date form is treated as absent).
pub fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

/// Whether the status means "slow down / try again".
pub fn retryable(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}
