//! FCM HTTP v1 (push design D2): a service account signs a JWT, Google's OAuth
//! endpoint swaps it for an access token, and one `messages:send` carries a
//! notification with the fixed text only.

use std::time::Duration;

use protocol::device_identity::{PushTokenKind, push_token_problem};
use serde_json::json;
use tokio::time::Instant;

use super::http::{Gate, backoff, client, retry_after, retryable};
use super::jwt::{now_secs, rs256};
use super::{COLLAPSE_KEY, FcmServiceAccount, Options, PushConfigError, WAKE_TEXT, WakeOutcome};

const SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";
/// An access token is renewed this long before Google says it expires.
const RENEW_EARLY: Duration = Duration::from_secs(300);

/// Sends wake pings through FCM.
pub struct FcmProvider {
    account: FcmServiceAccount,
    client: reqwest::Client,
    send_url: String,
    access: tokio::sync::Mutex<Option<(String, Instant)>>,
    gate: Gate,
    options: Options,
}

/// What to do after one HTTP answer.
enum Step {
    Done(WakeOutcome),
    /// The access token was refused: get a new one, retry once.
    Reauth,
    /// 429, 5xx or no answer: wait (`Retry-After` if given) and retry.
    Retry(Option<Duration>),
}

impl FcmProvider {
    /// A provider for `account`.
    pub fn new(account: FcmServiceAccount, options: &Options) -> Result<Self, PushConfigError> {
        let send_url = format!(
            "{}/v1/projects/{}/messages:send",
            options.fcm_base.trim_end_matches('/'),
            account.project_id
        );
        Ok(Self {
            account,
            client: client(options, false)?,
            send_url,
            access: tokio::sync::Mutex::new(None),
            gate: Gate::default(),
            options: options.clone(),
        })
    }

    /// One wake ping to `token` (pi-runtime: Push errors for the retries).
    pub async fn send(&self, token: &str) -> WakeOutcome {
        if push_token_problem(PushTokenKind::Fcm, token).is_some() {
            return WakeOutcome::TokenInvalid;
        }
        // Nothing but the token and the fixed text (message-delivery:
        // Content-free wake pings): no data, title, sender or count.
        let body = json!({
            "message": {
                "token": token,
                "notification": { "body": WAKE_TEXT },
                "android": {
                    "priority": "HIGH",
                    "ttl": "86400s",
                    "collapse_key": COLLAPSE_KEY,
                    "notification": { "tag": COLLAPSE_KEY },
                },
            }
        });
        let mut retries = 0;
        let mut reauthed = false;
        loop {
            self.gate.wait().await;
            let step = match self.access_token().await {
                Err(step) => step,
                Ok(access) => {
                    match self
                        .client
                        .post(&self.send_url)
                        .bearer_auth(access)
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
                Step::Done(outcome) => return outcome,
                Step::Reauth if !reauthed => {
                    reauthed = true;
                    *self.access.lock().await = None;
                }
                Step::Reauth => {
                    tracing::warn!(platform = "FCM", "FCM refused a fresh access token");
                    return WakeOutcome::Dropped;
                }
                Step::Retry(after) => {
                    if retries >= self.options.max_retries {
                        return WakeOutcome::Dropped;
                    }
                    let wait = backoff(self.options.backoff_base, retries, after);
                    self.gate.hold(wait);
                    if wait > self.options.max_wait {
                        return WakeOutcome::Dropped;
                    }
                    retries += 1;
                }
            }
        }
    }

    /// A cached access token, or a new one from the OAuth endpoint.
    async fn access_token(&self) -> Result<String, Step> {
        let mut access = self.access.lock().await;
        if let Some((token, until)) = access.as_ref()
            && Instant::now() < *until
        {
            return Ok(token.clone());
        }
        let iat = now_secs();
        let mut header = json!({ "alg": "RS256", "typ": "JWT" });
        if let Some(kid) = &self.account.private_key_id {
            header["kid"] = json!(kid);
        }
        let claims = json!({
            "iss": self.account.client_email,
            "scope": SCOPE,
            "aud": self.account.token_uri,
            "iat": iat,
            "exp": iat + 3600,
        });
        let assertion = rs256(&self.account.key, header, claims).map_err(|_| {
            tracing::error!(platform = "FCM", "signing the OAuth JWT failed");
            Step::Done(WakeOutcome::Dropped)
        })?;
        // The JWT is base64url and dots: URL-safe as it is.
        let form = format!(
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={assertion}"
        );
        let response = self
            .client
            .post(&self.account.token_uri)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(form)
            .send()
            .await
            .map_err(|_| Step::Retry(None))?;
        let status = response.status();
        if retryable(status) {
            return Err(Step::Retry(retry_after(response.headers())));
        }
        #[derive(serde::Deserialize)]
        struct TokenAnswer {
            access_token: String,
            expires_in: u64,
        }
        let answer: Option<TokenAnswer> = if status.is_success() {
            response.json().await.ok()
        } else {
            None
        };
        let Some(answer) = answer else {
            tracing::error!(
                platform = "FCM",
                status = status.as_u16(),
                "FCM sign-in failed"
            );
            return Err(Step::Done(WakeOutcome::Dropped));
        };
        let lifetime = Duration::from_secs(answer.expires_in)
            .saturating_sub(RENEW_EARLY)
            .max(Duration::from_secs(60));
        *access = Some((answer.access_token.clone(), Instant::now() + lifetime));
        Ok(answer.access_token)
    }
}

/// The FCM answer table (push design D4).
fn classify(status: reqwest::StatusCode, after: Option<Duration>, body: &[u8]) -> Step {
    if status.is_success() {
        return Step::Done(WakeOutcome::Sent);
    }
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Step::Reauth;
    }
    if retryable(status) {
        return Step::Retry(after);
    }
    let error: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let details = error["error"]["details"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let code = details
        .iter()
        .find_map(|d| d["errorCode"].as_str())
        .unwrap_or("");
    let token_field = details.iter().any(|d| {
        d["fieldViolations"]
            .as_array()
            .is_some_and(|v| v.iter().any(|f| f["field"] == "message.token"))
    });
    if matches!(code, "UNREGISTERED" | "SENDER_ID_MISMATCH")
        || (status == reqwest::StatusCode::BAD_REQUEST && token_field)
    {
        return Step::Done(WakeOutcome::TokenInvalid);
    }
    tracing::error!(
        platform = "FCM",
        status = status.as_u16(),
        error_code = code,
        "FCM refused the ping"
    );
    Step::Done(WakeOutcome::Dropped)
}
