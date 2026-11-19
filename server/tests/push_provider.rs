//! pi-runtime: Push provider interface. Without a real provider the server
//! logs one content-free wake ping (a redacted device id and platform only)
//! and sends nothing. This binary installs a global log capture, so it holds
//! one test.

mod support;

use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use server::runtime::server_keys::hex;
use support::harness::{TestPi, authed};
use support::messaging::{offline, say, wait_offline, world_on};
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
    fn lines(&self) -> Vec<String> {
        let bytes = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        String::from_utf8_lossy(&bytes)
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

// spec: pi-runtime/push-provider-interface
#[tokio::test]
async fn without_a_provider_the_pi_logs_one_content_free_wake_ping() {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .with_writer(move || writer.clone())
        .finish();
    tracing::subscriber::set_global_default(subscriber).check("log capture");

    let mut w = world_on(TestPi::start_with_default_push().await).await;
    w.conn
        .devices()
        .update_push_token(authed(
            w.phone.token(),
            pb::UpdatePushTokenRequest {
                push_token: Some(pb::PushToken {
                    platform: pb::PushPlatform::Fcm as i32,
                    token: "secret-fcm-token".to_owned(),
                }),
            },
        ))
        .await
        .check("UpdatePushToken");
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.pi, &w.phone).await;
    say(&mut w.admin, &mut w.admin_stream, b"content").await;

    let phone_device = server::runtime::redact(&w.phone.device_id).to_string();
    let pings = || -> Vec<String> {
        captured
            .lines()
            .into_iter()
            .filter(|line| line.contains("wake ping"))
            .collect()
    };
    let deadline = tokio::time::Instant::now() + support::messaging::WAIT;
    while pings().is_empty() {
        assert!(tokio::time::Instant::now() < deadline, "no wake ping");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // No second one comes.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let pings = pings();
    assert_eq!(pings.len(), 1, "{pings:?}");
    let line = &pings[0];
    assert!(line.contains(&phone_device), "{line}");
    assert!(line.contains("PUSH_PLATFORM_FCM"), "{line}");
    // Nothing else: no raw device id, token, sender, group or content.
    assert!(!line.contains(&hex(&w.phone.device_id)), "{line}");
    assert!(!line.contains("secret-fcm-token"), "{line}");
    assert!(!line.contains(&hex(&w.admin.device_id)), "{line}");
    assert!(!line.contains(&hex(support::messaging::GROUP_ID)), "{line}");
    assert!(!line.contains("the-one-group"), "{line}");
    assert!(
        w.pi.pushes.sent().is_empty(),
        "the test recorder is not in use"
    );
}
