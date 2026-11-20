//! server-runtime: Push provider interface, Push errors; message-delivery:
//! Content-free wake pings. The APNs provider against fake production and
//! sandbox APNs servers (push design D3, D4).

mod push_support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use push_support::{
    FakeServer, Reply, apns_key, apns_token, credentials_dir, options, verify_es256, write_key,
};
use server::pb;
use server::push::{Credentials, Providers, PushProvider, PushSettings, WakeOutcome};

const DEVICE: [u8; 16] = [9; 16];

struct Apns {
    production: FakeServer,
    sandbox: FakeServer,
    push: Providers,
    public: Vec<u8>,
}

async fn apns() -> Apns {
    let production = FakeServer::start().await;
    let sandbox = FakeServer::start().await;
    let (pem, public) = apns_key();
    let dir = credentials_dir();
    write_key(dir.path(), "AuthKey_ABC123DEFG.p8", &pem);
    let credentials = Credentials::load(dir.path()).expect("credentials");
    let settings = PushSettings::from_toml(
        r#"
credentials_dir = "."
apns_team_id = "XMVHK3YASX"
apns_topic = "com.purplechaamba.vyajena"
"#,
        dir.path(),
    )
    .expect("settings");
    let push = Providers::new(
        credentials,
        &settings,
        &options(&production, &production, &sandbox),
    )
    .expect("providers");
    Apns {
        production,
        sandbox,
        push,
        public,
    }
}

fn token(t: &str) -> pb::PushToken {
    pb::PushToken {
        platform: pb::PushPlatform::Apns as i32,
        token: t.to_owned(),
    }
}

fn reason(status: u16, reason: &str) -> Reply {
    Reply::new(status, &format!(r#"{{"reason":"{reason}"}}"#))
}

// spec: server-runtime/push-provider-interface
// spec: message-delivery/content-free-wake-pings
#[tokio::test]
async fn real_apns_ping_is_an_http2_alert_with_only_the_fixed_text() {
    let a = apns().await;
    assert!(a.push.has_apns() && !a.push.has_fcm());
    let t = apns_token(0xab);
    assert_eq!(a.push.wake(DEVICE, token(&t)).await, WakeOutcome::Sent);
    assert_eq!(a.push.wake(DEVICE, token(&t)).await, WakeOutcome::Sent);

    let sent = a.production.requests();
    assert_eq!(sent.len(), 2);
    assert!(a.sandbox.requests().is_empty());
    let req = &sent[0];
    assert_eq!(req.method, "POST");
    assert_eq!(req.path, format!("/3/device/{t}"));
    assert_eq!(req.version, hyper::Version::HTTP_2);
    assert_eq!(req.header("apns-push-type"), Some("alert"));
    assert_eq!(req.header("apns-priority"), Some("10"));
    assert_eq!(req.header("apns-topic"), Some("com.purplechaamba.vyajena"));
    assert_eq!(req.header("apns-collapse-id"), Some("wake"));
    let expiration: u64 = req
        .header("apns-expiration")
        .and_then(|v| v.parse().ok())
        .expect("apns-expiration");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    assert!(expiration > now && expiration <= now + 86_400 + 5);
    assert_eq!(
        req.json(),
        serde_json::json!({ "aps": { "alert": { "body": "New message" }, "sound": "default" } })
    );
    // The provider token: ES256, the key id from the file name, the team.
    let jwt = req
        .header("authorization")
        .and_then(|v| v.strip_prefix("bearer "))
        .expect("bearer");
    let (header, claims) = verify_es256(jwt, &a.public);
    assert_eq!(header["alg"], "ES256");
    assert_eq!(header["kid"], "ABC123DEFG");
    assert_eq!(claims["iss"], "XMVHK3YASX");
    assert!(claims["iat"].as_u64().is_some());
    // Reused, not re-signed per ping.
    assert_eq!(sent[1].header("authorization"), req.header("authorization"));
}

// spec: server-runtime/push-errors
#[tokio::test]
async fn a_sandbox_token_is_found_and_tried_there_first() {
    let a = apns().await;
    a.production.respond(|_| reason(400, "BadDeviceToken"));
    let t = apns_token(1);
    assert_eq!(a.push.wake(DEVICE, token(&t)).await, WakeOutcome::Sent);
    assert_eq!(a.production.requests().len(), 1);
    assert_eq!(a.sandbox.requests().len(), 1);
    assert_eq!(a.push.wake(DEVICE, token(&t)).await, WakeOutcome::Sent);
    assert_eq!(a.production.requests().len(), 1, "sandbox first now");
    assert_eq!(a.sandbox.requests().len(), 2);
}

// spec: server-runtime/push-errors
#[tokio::test]
async fn apns_answers_that_clear_the_token_and_ones_that_do_not() {
    // BadDeviceToken on both hosts.
    let a = apns().await;
    a.production.respond(|_| reason(400, "BadDeviceToken"));
    a.sandbox.respond(|_| reason(400, "BadDeviceToken"));
    assert_eq!(
        a.push.wake(DEVICE, token(&apns_token(2))).await,
        WakeOutcome::TokenInvalid
    );

    let cases = [
        (reason(410, "Unregistered"), WakeOutcome::TokenInvalid),
        (
            reason(400, "DeviceTokenNotForTopic"),
            WakeOutcome::TokenInvalid,
        ),
        (reason(400, "BadTopic"), WakeOutcome::Dropped),
        (reason(413, "PayloadTooLarge"), WakeOutcome::Dropped),
    ];
    for (reply, want) in cases {
        let a = apns().await;
        let answer = reply.clone();
        a.production.respond(move |_| answer.clone());
        assert_eq!(
            a.push.wake(DEVICE, token(&apns_token(3))).await,
            want,
            "{reply:?}"
        );
        assert_eq!(a.production.requests().len(), 1);
        assert!(
            a.sandbox.requests().is_empty(),
            "only BadDeviceToken tries the sandbox"
        );
    }

    // A token that is not hex never reaches APNs (it would be in the path).
    let a = apns().await;
    assert_eq!(
        a.push.wake(DEVICE, token("../../3/device/x")).await,
        WakeOutcome::TokenInvalid
    );
    assert!(a.production.requests().is_empty());
}

// spec: server-runtime/push-errors
#[tokio::test]
async fn an_expired_provider_token_is_re_signed_once_and_5xx_retries() {
    let a = apns().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    a.production
        .respond(move |_| match seen.fetch_add(1, Ordering::SeqCst) {
            0 => reason(403, "ExpiredProviderToken"),
            1 => reason(503, "ServiceUnavailable"),
            _ => Reply::new(200, ""),
        });
    assert_eq!(
        a.push.wake(DEVICE, token(&apns_token(4))).await,
        WakeOutcome::Sent
    );
    let sent = a.production.requests();
    assert_eq!(sent.len(), 3);
    assert_ne!(
        sent[0].header("authorization"),
        sent[1].header("authorization"),
        "a new JWT after ExpiredProviderToken"
    );

    let a = apns().await;
    a.production
        .respond(|_| reason(403, "InvalidProviderToken"));
    assert_eq!(
        a.push.wake(DEVICE, token(&apns_token(5))).await,
        WakeOutcome::Dropped
    );
    assert_eq!(a.production.requests().len(), 2);
}

// spec: server-runtime/push-provider-interface
#[tokio::test]
async fn only_apns_configured_an_fcm_token_is_only_logged() {
    let a = apns().await;
    let fcm = pb::PushToken {
        platform: pb::PushPlatform::Fcm as i32,
        token: "bob-android".into(),
    };
    assert_eq!(a.push.wake(DEVICE, fcm).await, WakeOutcome::Sent);
    assert!(a.production.requests().is_empty() && a.sandbox.requests().is_empty());
}
