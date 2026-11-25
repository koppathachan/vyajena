//! message-delivery: Content-free wake pings. The server pings a device that has
//! no open stream and has a push token when it adds an entry for it, at most
//! once per device per 60 seconds, and never a connected device.

mod support;

use std::time::Duration;

use support::clients::TestDevice;
use support::harness::{Conn, TestServer, authed};
use support::messaging::{Stream, commit_envelope, offline, say, wait_offline, world};
use support::{Check, pb};

async fn set_token(conn: &Conn, device: &TestDevice, token: &str) {
    conn.devices()
        .update_push_token(authed(
            device.token(),
            pb::UpdatePushTokenRequest {
                push_token: Some(pb::PushToken {
                    platform: pb::PushPlatform::Fcm as i32,
                    token: token.to_owned(),
                }),
            },
        ))
        .await
        .check("UpdatePushToken");
}

/// Waits for `device` to have `expected` pings, then checks no more come.
async fn pings_settle(server: &TestServer, device: &TestDevice, expected: usize) {
    let deadline = tokio::time::Instant::now() + support::messaging::WAIT;
    while server.pushes.count(&device.device_id) < expected {
        assert!(
            tokio::time::Instant::now() < deadline,
            "{} of {expected} pings",
            server.pushes.count(&device.device_id)
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // No more come.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(server.pushes.count(&device.device_id), expected);
}

// spec: message-delivery/content-free-wake-pings
#[tokio::test]
async fn offline_device_gets_one_ping_with_nothing_but_its_token() {
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    set_token(&w.conn, &w.laptop, "laptop-fcm-token").await;
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;

    say(&mut w.admin, &mut w.admin_stream, b"wake up").await;
    pings_settle(&w.server, &w.phone, 1).await;
    let (device_id, token) = w.server.pushes.sent()[0].clone();
    assert_eq!(device_id, w.phone.device_id);
    assert_eq!(token.token, "phone-fcm-token");
    assert_eq!(token.platform, pb::PushPlatform::Fcm as i32);
    // The connected laptop gets the message on its stream, and no ping.
    w.laptop_stream.envelope().await;
    pings_settle(&w.server, &w.laptop, 0).await;
}

// spec: message-delivery/content-free-wake-pings
#[tokio::test]
async fn at_most_one_ping_per_device_per_60_seconds() {
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;

    say(&mut w.admin, &mut w.admin_stream, b"one").await;
    pings_settle(&w.server, &w.phone, 1).await;
    w.server.clock.advance(Duration::from_secs(59));
    say(&mut w.admin, &mut w.admin_stream, b"two").await;
    pings_settle(&w.server, &w.phone, 1).await;
    w.server.clock.advance(Duration::from_secs(2));
    say(&mut w.admin, &mut w.admin_stream, b"three").await;
    pings_settle(&w.server, &w.phone, 2).await;
}

// spec: message-delivery/content-free-wake-pings
#[tokio::test]
async fn connected_device_gets_the_message_and_no_ping() {
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    say(&mut w.admin, &mut w.admin_stream, b"on the stream").await;
    let got = w.phone_stream.envelope().await;
    assert_eq!(w.phone.read_message(&got.ciphertext), b"on the stream");
    pings_settle(&w.server, &w.phone, 0).await;
}

// spec: message-delivery/content-free-wake-pings
#[tokio::test]
async fn control_only_backlog_fires_no_ping_but_a_message_does() {
    // push-kind-aware: Only an application backlog alerts an offline device.
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;

    // The admin commits a leaf update: a control-only `COMMIT` is queued to the
    // offline phone (no Welcome, no application entry).
    let epoch = w.admin.epoch();
    let commit = w.admin.self_update();
    w.admin_stream
        .accepted(commit_envelope(epoch, commit, None))
        .await;
    w.admin.confirm();
    // A control-only backlog must not alert the offline device.
    pings_settle(&w.server, &w.phone, 0).await;

    // A real message to the same offline device does fire exactly one ping.
    say(&mut w.admin, &mut w.admin_stream, b"a real message").await;
    pings_settle(&w.server, &w.phone, 1).await;
}

// spec: message-delivery/content-free-wake-pings
#[tokio::test]
async fn device_without_a_push_token_gets_no_ping() {
    let mut w = world().await;
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;
    say(&mut w.admin, &mut w.admin_stream, b"queued only").await;
    pings_settle(&w.server, &w.phone, 0).await;
    // It gets the entry when it connects.
    let mut stream = Stream::open(&w.conn, &w.phone).await;
    let got = stream.envelope().await;
    assert_eq!(w.phone.read_message(&got.ciphertext), b"queued only");
}
