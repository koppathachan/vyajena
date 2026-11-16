//! APNs over HTTP/2 with a token-based `.p8` key (push design D3): the Pi signs
//! its own ES256 provider JWT and sends a visible alert with the fixed text.
//! Production first; only `BadDeviceToken` there tries the sandbox (Xcode
//! builds), and the token is cleared only if both refuse it.

use std::collections::HashSet;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use protocol::device_identity::{PushTokenKind, push_token_problem};
use serde_json::json;
use tokio::time::Instant;

use super::http::{Gate, backoff, client, retry_after, retryable};
use super::jwt::{es256, now_secs};
use super::{ApnsKey, COLLAPSE_KEY, Options, PushConfigError, WAKE_TEXT, WakeOutcome};

/// A provider JWT is reused this long (Apple: refresh at most every 20
/// minutes, valid for 60).
const JWT_LIFETIME: Duration = Duration::from_secs(50 * 60);
/// `apns-expiration`: a ping older than this is not delivered.
const PING_TTL_SECS: u64 = 86_400;

/// Sends wake pings through APNs.
pub struct ApnsProvider {
    key: ApnsKey,
    team_id: String,
    topic: String,
    client: reqwest::Client,
    jwt: Mutex<Option<(String, Instant)>>,
    /// Tokens the sandbox accepted (Xcode debug builds), tried there first.
    sandbox_tokens: Mutex<HashSet<String>>,
    gate: Gate,
    options: Options,
}

/// What one host made of the ping.
enum Answer {
    Done(WakeOutcome),
    BadDeviceToken,
}

enum Step {
    Done(Answer),
    Reauth,
    Retry(Option<Duration>),
}

impl ApnsProvider {
    /// A provider for `key`, team `team_id`, app `topic`.
    pub fn new(
        key: ApnsKey,
        team_id: String,
        topic: String,
        options: &Options,
    ) -> Result<Self, PushConfigError> {
        Ok(Self {
            key,
            team_id,
            topic,
            client: client(options, true)?,
            jwt: Mutex::new(None),
            sandbox_tokens: Mutex::new(HashSet::new()),
            gate: Gate::default(),
            options: options.clone(),
        })
    }

    /// One wake ping to `token`.
    pub async fn send(&self, token: &str) -> WakeOutcome {
        // The token goes into the request path: hex only.
        if push_token_problem(PushTokenKind::Apns, token).is_some() {
            return WakeOutcome::TokenInvalid;
        }
        let sandbox_first = self
            .sandbox_tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(token);
        let production = self.options.apns_production.as_str();
        let sandbox = self.options.apns_sandbox.as_str();
        let hosts: &[(&str, bool)] = if sandbox_first {
            &[(sandbox, true), (production, false)]
        } else {
            &[(production, false), (sandbox, true)]
        };
        for (base, is_sandbox) in hosts {
            match self.send_to(base, token).await {
                Answer::Done(outcome) => {
                    let mut known = self
                        .sandbox_tokens
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    if outcome == WakeOutcome::Sent && *is_sandbox {
                        known.insert(token.to_owned());
                    } else if outcome == WakeOutcome::Sent || outcome == WakeOutcome::TokenInvalid {
                        known.remove(token);
                    }
                    return outcome;
                }
                Answer::BadDeviceToken => continue,
            }
        }
        // Both hosts said BadDeviceToken (pi-runtime: Push errors).
        self.sandbox_tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(token);
        WakeOutcome::TokenInvalid
    }

    async fn send_to(&self, base: &str, token: &str) -> Answer {
        let url = format!("{}/3/device/{token}", base.trim_end_matches('/'));
        // A visible alert with the fixed text only (owner, 2026-11-16).
        let body = json!({ "aps": { "alert": { "body": WAKE_TEXT }, "sound": "default" } });
        let mut retries = 0;
        let mut reauthed = false;
        loop {
            self.gate.wait().await;
            let step = match self.provider_jwt() {
                None => Step::Done(Answer::Done(WakeOutcome::Dropped)),
                Some(jwt) => {
                    match self
                        .client
                        .post(&url)
                        .header(reqwest::header::AUTHORIZATION, format!("bearer {jwt}"))
                        .header("apns-push-type", "alert")
                        .header("apns-priority", "10")
                        .header("apns-expiration", (now_secs() + PING_TTL_SECS).to_string())
                        .header("apns-collapse-id", COLLAPSE_KEY)
                        .header("apns-topic", &self.topic)
                        .json(&body)
                        .send()
                        .await
                    {
                        Err(_) => Step::Retry(None),
                        Ok(response) => {
                            let status = response.status();
                            let after = retry_after(response.headers());
                            let bytes = response.bytes().await.unwrap_or_default();
                            classify(status, after, &bytes)
                        }
                    }
                }
            };
            match step {
                Step::Done(answer) => return answer,
                Step::Reauth if !reauthed => {
                    reauthed = true;
                    *self.jwt.lock().unwrap_or_else(PoisonError::into_inner) = None;
                }
                Step::Reauth => {
                    tracing::warn!(platform = "APNS", "APNs refused a fresh provider token");
                    return Answer::Done(WakeOutcome::Dropped);
                }
                Step::Retry(after) => {
                    if retries >= self.options.max_retries {
                        return Answer::Done(WakeOutcome::Dropped);
                    }
                    let wait = backoff(self.options.backoff_base, retries, after);
                    self.gate.hold(wait);
                    if wait > self.options.max_wait {
                        return Answer::Done(WakeOutcome::Dropped);
                    }
                    retries += 1;
                }
            }
        }
    }

    /// The cached provider JWT, or a newly signed one.
    fn provider_jwt(&self) -> Option<String> {
        let mut jwt = self.jwt.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((token, until)) = jwt.as_ref()
            && Instant::now() < *until
        {
            return Some(token.clone());
        }
        let signed = es256(
            &self.key.key,
            json!({ "alg": "ES256", "kid": self.key.key_id }),
            json!({ "iss": self.team_id, "iat": now_secs() }),
        );
        match signed {
            Ok(token) => {
                *jwt = Some((token.clone(), Instant::now() + JWT_LIFETIME));
                Some(token)
            }
            Err(_) => {
                tracing::error!(platform = "APNS", "signing the APNs provider token failed");
                None
            }
        }
    }
}

/// The APNs answer table (push design D4).
fn classify(status: reqwest::StatusCode, after: Option<Duration>, body: &[u8]) -> Step {
    if status.is_success() {
        return Step::Done(Answer::Done(WakeOutcome::Sent));
    }
    if retryable(status) {
        return Step::Retry(after);
    }
    let error: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let reason = error["reason"].as_str().unwrap_or("");
    match (status.as_u16(), reason) {
        (410, _) | (400, "DeviceTokenNotForTopic") => {
            Step::Done(Answer::Done(WakeOutcome::TokenInvalid))
        }
        (400, "BadDeviceToken") => Step::Done(Answer::BadDeviceToken),
        (403, "ExpiredProviderToken" | "InvalidProviderToken") => Step::Reauth,
        _ => {
            tracing::error!(
                platform = "APNS",
                status = status.as_u16(),
                reason,
                "APNs refused the ping"
            );
            Step::Done(Answer::Done(WakeOutcome::Dropped))
        }
    }
}
