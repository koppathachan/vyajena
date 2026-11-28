//! message-delivery: Content-free wake pings. The server pings a device that has
//! no open stream and has a push token when it adds an entry for it, at most
//! once per device per 60 seconds, and never a connected device.

mod support;

use std::time::Duration;

use support::clients::TestDevice;
use support::harness::{Conn, TestServer, authed};
use support::messaging::{Stream, commit_envelope, offline, say, say_silent, wait_offline, world};
use support::{Check, pb};

const MINUTE: Duration = Duration::from_secs(61);

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
async fn message_pings_are_floored_at_10_seconds() {
    // A newly-queued message is floored at NEW_MESSAGE_PING_MIN_INTERVAL (10 s), not
    // the 60 s nudge interval, so a real message is not held back for a whole minute.
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;

    say(&mut w.admin, &mut w.admin_stream, b"one").await;
    pings_settle(&w.server, &w.phone, 1).await;
    // A second message within 10 seconds collapses: no new ping.
    w.server.clock.advance(Duration::from_secs(9));
    say(&mut w.admin, &mut w.admin_stream, b"two").await;
    pings_settle(&w.server, &w.phone, 1).await;
    // Past the 10-second message floor, it pings again.
    w.server.clock.advance(Duration::from_secs(2));
    say(&mut w.admin, &mut w.admin_stream, b"three").await;
    pings_settle(&w.server, &w.phone, 2).await;
}

// spec: message-delivery/content-free-wake-pings
#[tokio::test]
async fn a_recent_nudge_does_not_starve_a_new_message() {
    // The failure this fixes: a real message queued within 60 s of a catch-up or
    // own-access nudge used to be suppressed. A nudge must not consume the message
    // interval.
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;

    // A first message pings and stays queued (the phone never acks).
    say(&mut w.admin, &mut w.admin_stream, b"first").await;
    pings_settle(&w.server, &w.phone, 1).await;

    // 65 s later the catch-up sweep sends a nudge for the still-pending backlog.
    w.server.clock.advance(Duration::from_secs(65));
    w.server.sweep().await;
    pings_settle(&w.server, &w.phone, 2).await;

    // 15 s after the nudge — within its 60 s window — a new message still pings.
    // Under the old single-budget limiter the nudge would have suppressed it.
    w.server.clock.advance(Duration::from_secs(15));
    say(&mut w.admin, &mut w.admin_stream, b"second").await;
    pings_settle(&w.server, &w.phone, 3).await;
}

// spec: message-delivery/content-free-wake-pings
#[tokio::test]
async fn nudge_pings_keep_the_60_second_cadence() {
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;

    // A message leaves a backlog and pings once.
    say(&mut w.admin, &mut w.admin_stream, b"a message").await;
    pings_settle(&w.server, &w.phone, 1).await;

    // A sweep more than 60 s later sends a nudge for the backlog.
    w.server.clock.advance(Duration::from_secs(65));
    w.server.sweep().await;
    pings_settle(&w.server, &w.phone, 2).await;
    // Another sweep less than 60 s after the nudge sends none.
    w.server.clock.advance(Duration::from_secs(30));
    w.server.sweep().await;
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
async fn silent_app_envelope_fires_no_ping_but_a_message_does() {
    // push-silent-app-envelopes: Only a user-visible message alerts an offline
    // device. A `silent` APPLICATION envelope (a profile update, a group-settings
    // change or a delete tombstone) is content the receiver applies with nothing
    // to show, so it must not fire a visible wake push.
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;

    // A silent application envelope is queued to the offline phone.
    say_silent(&mut w.admin, &mut w.admin_stream, b"a profile update").await;
    pings_settle(&w.server, &w.phone, 0).await;

    // A real (non-silent) message to the same offline device fires exactly one.
    say(&mut w.admin, &mut w.admin_stream, b"a real message").await;
    pings_settle(&w.server, &w.phone, 1).await;
}

// spec: message-delivery/content-free-wake-pings
#[tokio::test]
async fn a_silent_only_backlog_is_not_swept() {
    // push-silent-app-envelopes: a silent-only backlog neither fires a catch-up
    // ping nor spends the catch-up budget, exactly as a control-only one.
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;

    // Only a silent application envelope is queued to the offline phone.
    say_silent(&mut w.admin, &mut w.admin_stream, b"a profile update").await;
    // Neither the write path nor repeated sweeps wake it.
    w.server.clock.advance(MINUTE);
    w.server.sweep().await;
    w.server.clock.advance(MINUTE);
    w.server.sweep().await;
    pings_settle(&w.server, &w.phone, 0).await;

    // A real message makes it notifiable again: it is woken exactly once.
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
