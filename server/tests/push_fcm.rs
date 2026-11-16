//! pi-runtime: Push provider interface, Push errors; message-delivery:
//! Content-free wake pings. The FCM provider against fake Google OAuth and
//! FCM servers (push design D2, D4).

mod push_support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use push_support::{
    FakeServer, Reply, assertion, credentials_dir, options, rsa_key, service_account_json,
    verify_rs256, write_key,
};
use serde_json::json;
use server::pb;
use server::push::{
    Credentials, FCM_SERVICE_ACCOUNT_FILE, Options, Providers, PushProvider, PushSettings,
    WakeOutcome,
};

const SEND: &str = "/v1/projects/vyajena-test/messages:send";
const DEVICE: [u8; 16] = [7; 16];

/// FCM providers on one fake that is both OAuth and FCM.
async fn fcm() -> (FakeServer, Providers) {
    let fake = FakeServer::start().await;
    let opts = options(&fake, &fake, &fake);
    (fake.clone(), providers(&fake, &opts))
}

fn providers(oauth: &FakeServer, opts: &Options) -> Providers {
    let dir = credentials_dir();
    write_key(
        dir.path(),
        FCM_SERVICE_ACCOUNT_FILE,
        &service_account_json(oauth),
    );
    let credentials = Credentials::load(dir.path()).expect("credentials");
    let settings = PushSettings::from_toml(
        r#"
credentials_dir = "."
apns_team_id = "TEAM123456"
apns_topic = "com.purplechaamba.vyajena"
"#,
        dir.path(),
    )
    .expect("settings");
    Providers::new(credentials, &settings, opts).expect("providers")
}

fn token(t: &str) -> pb::PushToken {
    pb::PushToken {
        platform: pb::PushPlatform::Fcm as i32,
        token: t.to_owned(),
    }
}

fn fcm_error(status: u16, code: &str, field: Option<&str>) -> Reply {
    let mut details = vec![json!({
        "@type": "type.googleapis.com/google.firebase.fcm.v1.FcmError",
        "errorCode": code,
    })];
    if let Some(field) = field {
        details.push(json!({
            "@type": "type.googleapis.com/google.rpc.BadRequest",
            "fieldViolations": [{ "field": field, "description": "bad" }],
        }));
    }
    Reply::new(
        status,
        &json!({ "error": { "code": status, "message": "x", "status": code, "details": details } })
            .to_string(),
    )
}

// spec: pi-runtime/push-provider-interface
// spec: message-delivery/content-free-wake-pings
#[tokio::test]
async fn real_fcm_ping_signs_in_and_sends_only_the_fixed_text() {
    let (fake, push) = fcm().await;
    assert!(push.has_fcm() && !push.has_apns());

    assert_eq!(
        push.wake(DEVICE, token("bob-phone")).await,
        WakeOutcome::Sent
    );
    assert_eq!(
        push.wake(DEVICE, token("bob-phone")).await,
        WakeOutcome::Sent
    );

    // One sign-in: a JWT bearer grant signed with the service account's key.
    let logins = fake.requests_to("/token");
    assert_eq!(logins.len(), 1, "the access token is cached");
    let login = &logins[0];
    assert_eq!(login.method, "POST");
    assert!(login.text().starts_with(
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion="
    ));
    let (header, claims) = verify_rs256(&assertion(&login.text()), &rsa_key().1);
    assert_eq!(header["alg"], "RS256");
    assert_eq!(header["kid"], "kid-1");
    assert_eq!(
        claims["iss"],
        "vyajena-push@vyajena-test.iam.gserviceaccount.com"
    );
    assert_eq!(
        claims["scope"],
        "https://www.googleapis.com/auth/firebase.messaging"
    );
    assert_eq!(claims["aud"], format!("{}/token", fake.base));
    assert_eq!(
        claims["exp"].as_u64().unwrap_or(0) - claims["iat"].as_u64().unwrap_or(0),
        3600
    );

    // Two sends with the access token, each exactly this body.
    let sends = fake.requests_to(SEND);
    assert_eq!(sends.len(), 2);
    for send in &sends {
        assert_eq!(send.method, "POST");
        assert_eq!(send.header("authorization"), Some("Bearer at-1"));
        assert_eq!(
            send.json(),
            json!({ "message": {
                "token": "bob-phone",
                "notification": { "body": "New message" },
                "android": {
                    "priority": "HIGH",
                    "ttl": "86400s",
                    "collapse_key": "wake",
                    "notification": { "tag": "wake" },
                },
            }})
        );
    }
}

// spec: pi-runtime/push-errors
#[tokio::test]
async fn fcm_answers_that_clear_the_token_and_ones_that_do_not() {
    let cases = [
        (
            fcm_error(404, "UNREGISTERED", None),
            WakeOutcome::TokenInvalid,
        ),
        (
            fcm_error(403, "SENDER_ID_MISMATCH", None),
            WakeOutcome::TokenInvalid,
        ),
        (
            fcm_error(400, "INVALID_ARGUMENT", Some("message.token")),
            WakeOutcome::TokenInvalid,
        ),
        // A payload problem is not the token's fault: nothing is cleared.
        (
            fcm_error(400, "INVALID_ARGUMENT", Some("message.android.ttl")),
            WakeOutcome::Dropped,
        ),
        // A wrong project is a 404 without UNREGISTERED: nothing is cleared.
        (
            Reply::new(404, r#"{"error":{"code":404,"status":"NOT_FOUND"}}"#),
            WakeOutcome::Dropped,
        ),
        (
            fcm_error(403, "PERMISSION_DENIED", None),
            WakeOutcome::Dropped,
        ),
    ];
    for (reply, want) in cases {
        let (fake, push) = fcm().await;
        let answer = reply.clone();
        fake.respond(move |req| {
            if req.path == SEND {
                answer.clone()
            } else {
                push_support::default_reply(req)
            }
        });
        assert_eq!(push.wake(DEVICE, token("t")).await, want, "{reply:?}");
        assert_eq!(fake.requests_to(SEND).len(), 1, "no retry for {reply:?}");
    }
}

// spec: pi-runtime/push-errors
#[tokio::test]
async fn fcm_overloaded_backs_off_doubling_and_retries() {
    let (fake, push) = fcm().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    fake.respond(move |req| {
        if req.path != SEND {
            return push_support::default_reply(req);
        }
        match seen.fetch_add(1, Ordering::SeqCst) {
            0 | 1 => Reply::new(503, "{}"),
            _ => Reply::new(200, r#"{"name":"projects/vyajena-test/messages/0:1"}"#),
        }
    });
    let started = Instant::now();
    assert_eq!(push.wake(DEVICE, token("t")).await, WakeOutcome::Sent);
    // 50 ms, then 100 ms (the test's base; 1 s, 2 s on the Pi).
    assert!(
        started.elapsed() >= Duration::from_millis(150),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(fake.requests_to(SEND).len(), 3);
}

// spec: pi-runtime/push-errors
#[tokio::test]
async fn fcm_gives_up_after_three_retries_and_honours_retry_after() {
    let (fake, push) = fcm().await;
    fake.respond(|req| {
        if req.path == SEND {
            Reply::new(500, "{}")
        } else {
            push_support::default_reply(req)
        }
    });
    assert_eq!(push.wake(DEVICE, token("t")).await, WakeOutcome::Dropped);
    assert_eq!(
        fake.requests_to(SEND).len(),
        4,
        "the first try and 3 retries"
    );

    let (fake, push) = fcm().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    fake.respond(move |req| {
        if req.path != SEND {
            return push_support::default_reply(req);
        }
        if seen.fetch_add(1, Ordering::SeqCst) == 0 {
            Reply::new(429, "{}").with_header("retry-after", "1")
        } else {
            Reply::new(200, "{}")
        }
    });
    let push = Arc::new(push);
    let started = Instant::now();
    let first = tokio::spawn({
        let push = push.clone();
        async move { push.wake(DEVICE, token("a")).await }
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    // While FCM is backed off, another ping waits too.
    let second = push.wake([8; 16], token("b")).await;
    assert_eq!(second, WakeOutcome::Sent);
    assert!(
        started.elapsed() >= Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(first.await.expect("join"), WakeOutcome::Sent);
    assert_eq!(fake.requests_to(SEND).len(), 3);
}

// spec: pi-runtime/push-errors
#[tokio::test]
async fn a_refused_access_token_is_replaced_once() {
    let (fake, push) = fcm().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    fake.respond(move |req| {
        if req.path != SEND {
            return push_support::default_reply(req);
        }
        if seen.fetch_add(1, Ordering::SeqCst) == 0 {
            Reply::new(401, r#"{"error":{"code":401,"status":"UNAUTHENTICATED"}}"#)
        } else {
            Reply::new(200, "{}")
        }
    });
    assert_eq!(push.wake(DEVICE, token("t")).await, WakeOutcome::Sent);
    assert_eq!(fake.requests_to("/token").len(), 2);
    assert_eq!(fake.requests_to(SEND).len(), 2);

    // Always 401: one re-sign-in, then the ping is dropped.
    fake.respond(|req| {
        if req.path == SEND {
            Reply::new(401, "{}")
        } else {
            push_support::default_reply(req)
        }
    });
    assert_eq!(push.wake(DEVICE, token("t")).await, WakeOutcome::Dropped);
    assert_eq!(fake.requests_to(SEND).len(), 4);
}

// spec: pi-runtime/push-provider-interface
#[tokio::test]
async fn an_apns_token_without_apns_credentials_is_only_logged() {
    let (fake, push) = fcm().await;
    let apns = pb::PushToken {
        platform: pb::PushPlatform::Apns as i32,
        token: push_support::apns_token(1),
    };
    assert_eq!(push.wake(DEVICE, apns).await, WakeOutcome::Sent);
    assert!(fake.requests().is_empty(), "nothing sent");
}
