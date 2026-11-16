//! message-delivery: Content-free wake pings and pi-runtime: Push errors, end
//! to end: a test Pi's pings go to the real providers pointed at fake APNs and
//! FCM servers. This binary installs a global log capture, so it holds one
//! test: push logs name only the device id, platform and outcome.

mod push_support;
mod support;

use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use push_support::{
    FakeServer, Reply, apns_key, apns_token, credentials_dir, options, service_account_json,
    write_key,
};
use server::push::{Credentials, Providers, PushSettings};
use server::runtime::server_keys::hex;
use support::harness::authed;
use support::messaging::{GROUP_ID, WAIT, offline, say, wait_offline, world};
use support::{Check, pb};

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Captured {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap_or_else(PoisonError::into_inner)).into_owned()
    }
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + WAIT;
    while !done() {
        assert!(tokio::time::Instant::now() < deadline, "{what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// spec: message-delivery/content-free-wake-pings
// spec: pi-runtime/push-errors
#[tokio::test]
async fn bobs_closed_iphone_gets_only_new_message_and_logs_name_nothing_else() {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(move || writer.clone())
        .finish();
    tracing::subscriber::set_global_default(subscriber).check("log capture");

    // Real providers for both platforms, on fakes.
    let fcm = FakeServer::start().await;
    let apns = FakeServer::start().await;
    let sandbox = FakeServer::start().await;
    fcm.respond(|req| {
        if req.path == "/token" {
            push_support::default_reply(req)
        } else {
            Reply::new(
                200,
                r#"{"name":"projects/vyajena-test/messages/0:secret-message-name"}"#,
            )
        }
    });
    apns.respond(|_| Reply::new(200, "").with_header("apns-id", "secret-apns-id"));
    let dir = credentials_dir();
    write_key(
        dir.path(),
        "fcm-service-account.json",
        &service_account_json(&fcm),
    );
    write_key(dir.path(), "AuthKey_ABC123DEFG.p8", &apns_key().0);
    let settings = PushSettings::from_toml(
        "credentials_dir = \".\"\napns_team_id = \"XMVHK3YASX\"\napns_topic = \"com.purplechaamba.vyajena\"\n",
        dir.path(),
    )
    .check("settings");
    let providers = Providers::new(
        Credentials::load(dir.path()).check("creds"),
        &settings,
        &options(&fcm, &apns, &sandbox),
    )
    .check("providers");

    let mut w = world().await;
    w.pi.pushes.forward_to(Arc::new(providers));
    // Bob's iPhone (APNs) and Alice's Android laptop stand-in (FCM).
    let iphone_token = apns_token(0x5e);
    for (device, platform, token) in [
        (&w.phone, pb::PushPlatform::Apns, iphone_token.clone()),
        (
            &w.laptop,
            pb::PushPlatform::Fcm,
            "alice-fcm-token".to_owned(),
        ),
    ] {
        w.conn
            .devices()
            .update_push_token(authed(
                device.token(),
                pb::UpdatePushTokenRequest {
                    push_token: Some(pb::PushToken {
                        platform: platform as i32,
                        token: token.clone(),
                    }),
                },
            ))
            .await
            .check("UpdatePushToken");
    }
    w.phone_stream = offline(&w.conn, &w.phone).await;
    w.laptop_stream = offline(&w.conn, &w.laptop).await;
    wait_offline(&w.pi, &w.phone).await;
    wait_offline(&w.pi, &w.laptop).await;

    say(&mut w.admin, &mut w.admin_stream, b"dinner at 8?").await;
    until("an APNs ping", || !apns.requests().is_empty()).await;
    until("an FCM ping", || !fcm.requests_to("/v1/").is_empty()).await;

    // APNs got exactly one alert with only the fixed text.
    let alerts = apns.requests();
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].path, format!("/3/device/{iphone_token}"));
    assert_eq!(
        alerts[0].json(),
        serde_json::json!({ "aps": { "alert": { "body": "New message" }, "sound": "default" } })
    );
    let admin_id = hex(&w.admin.device_id);
    let group_hex = hex(GROUP_ID);
    for req in alerts.iter().chain(fcm.requests_to("/v1/").iter()) {
        let all = format!("{} {:?} {}", req.path, req.headers, req.text());
        for secret in [
            "dinner",
            "akhil",
            "bob",
            "alice@",
            &admin_id,
            &group_hex,
            "the-one-group",
        ] {
            assert!(
                !all.contains(secret),
                "{secret} reached the push service: {all}"
            );
        }
    }

    // FCM forgets Alice's token: the Pi clears it.
    fcm.respond(|req| {
        if req.path == "/token" {
            push_support::default_reply(req)
        } else {
            Reply::new(
                404,
                r#"{"error":{"code":404,"status":"NOT_FOUND","details":[{"errorCode":"UNREGISTERED"}]}}"#,
            )
        }
    });
    w.pi.clock.advance(Duration::from_secs(61));
    say(&mut w.admin, &mut w.admin_stream, b"second").await;
    let laptop = w.laptop.device_id;
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let token: Option<String> =
            w.pi.query(move |conn| {
                conn.query_row(
                    "SELECT push_token FROM devices WHERE device_id = ?1",
                    [laptop.as_slice()],
                    |row| row.get(0),
                )
            })
            .await;
        if token.is_none() {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "token not cleared");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Logs: device ids, platforms and outcomes; never tokens, ids of the
    // message at FCM/APNs, access tokens or response bodies.
    let logs = captured.text();
    assert!(logs.contains(&hex(&w.phone.device_id)), "{logs}");
    assert!(
        logs.contains("outcome=\"sent\"") || logs.contains("outcome=sent"),
        "{logs}"
    );
    assert!(logs.contains("token_invalid"), "{logs}");
    for secret in [
        iphone_token.as_str(),
        "alice-fcm-token",
        "secret-message-name",
        "secret-apns-id",
        "at-1",
        "UNREGISTERED",
        "dinner",
    ] {
        assert!(!logs.contains(secret), "{secret} in the logs:\n{logs}");
    }
}
