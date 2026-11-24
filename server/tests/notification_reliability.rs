//! notification-reliability: the server re-pings a device whose stream is lost
//! while it still has an undelivered backlog (D1), and keeps nudging an offline
//! device with a backlog on each sweep, rate-limited and bounded, with the
//! attempt budget kept durably so it survives a restart (D2, D5).

mod support;

use std::time::Duration;

use protocol::message_delivery::CATCH_UP_MAX;
use support::clients::TestDevice;
use support::harness::{Conn, TestServer, authed};
use support::messaging::{offline, reconnect, say, wait_offline, world};
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
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(server.pushes.count(&device.device_id), expected);
}

// spec: message-delivery/re-ping-on-stream-loss
#[tokio::test]
async fn a_backlog_at_stream_loss_gets_a_catch_up_ping() {
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    // Queued while the phone's stream is open (the zombie window): delivered on
    // the stream, so no store-time ping, and the phone never acks it.
    say(&mut w.admin, &mut w.admin_stream, b"while open").await;
    // The stream is lost with that entry still undelivered.
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;
    // The reap finds the backlog and sends one catch-up wake push.
    pings_settle(&w.server, &w.phone, 1).await;
    let (device_id, token) = w.server.pushes.sent()[0].clone();
    assert_eq!(device_id, w.phone.device_id);
    assert_eq!(token.token, "phone-fcm-token");
}

// spec: message-delivery/re-ping-on-stream-loss
#[tokio::test]
async fn a_stream_closing_with_no_backlog_gets_no_ping() {
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    // The phone is caught up (it acked its Welcome). Closing its stream must
    // not ping it: there is nothing undelivered.
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;
    pings_settle(&w.server, &w.phone, 0).await;
}

// spec: message-delivery/sweep-catch-up-ping
#[tokio::test]
async fn a_lost_ping_is_retried_by_the_sweep() {
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;
    // Offline when the message lands: one store-time wake push, which we treat
    // as lost (the phone stays offline with the message undelivered).
    say(&mut w.admin, &mut w.admin_stream, b"queued").await;
    pings_settle(&w.server, &w.phone, 1).await;
    // A later sweep sends another, within the one-per-60 s limit.
    w.server.clock.advance(MINUTE);
    w.server.sweep().await;
    pings_settle(&w.server, &w.phone, 2).await;
}

// spec: message-delivery/catch-up-stops-when-drained
#[tokio::test]
async fn catch_up_stops_once_the_backlog_drains() {
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;
    say(&mut w.admin, &mut w.admin_stream, b"m1").await;
    pings_settle(&w.server, &w.phone, 1).await;
    w.server.clock.advance(MINUTE);
    w.server.sweep().await;
    pings_settle(&w.server, &w.phone, 2).await;

    // The phone reconnects, catches up, acks and drains its queue.
    let mut phone_stream = reconnect(&w.conn, &mut w.phone).await;
    let env = phone_stream.envelope().await;
    assert_eq!(w.phone.read_message(&env.ciphertext), b"m1");
    phone_stream.ack(env.server_seq).await;
    phone_stream.sync().await;
    // It goes offline again with nothing left undelivered.
    drop(phone_stream);
    wait_offline(&w.server, &w.phone).await;

    // A later sweep requests no further catch-up push.
    w.server.clock.advance(MINUTE);
    w.server.sweep().await;
    pings_settle(&w.server, &w.phone, 2).await;
}

// spec: message-delivery/sweep-catch-up-ping
#[tokio::test]
async fn a_connected_device_is_never_swept_pinged() {
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    // The phone stays connected but leaves the message unacked (a backlog).
    say(&mut w.admin, &mut w.admin_stream, b"on the stream").await;
    w.server.clock.advance(MINUTE);
    w.server.sweep().await;
    // Connected: it receives on its stream and the sweep never pings it.
    pings_settle(&w.server, &w.phone, 0).await;
}

// spec: message-delivery/sweep-catch-up-ping
#[tokio::test]
async fn catch_up_is_bounded_then_stops() {
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;
    say(&mut w.admin, &mut w.admin_stream, b"m1").await;
    pings_settle(&w.server, &w.phone, 1).await;

    let mut expected = 1usize;
    for _ in 0..CATCH_UP_MAX {
        w.server.clock.advance(MINUTE);
        w.server.sweep().await;
        expected += 1;
        pings_settle(&w.server, &w.phone, expected).await;
    }
    // The bound is reached: further sweeps send nothing more.
    w.server.clock.advance(MINUTE);
    w.server.sweep().await;
    pings_settle(&w.server, &w.phone, expected).await;
}

// spec: message-delivery/catch-up-bound-survives-restart
#[tokio::test]
async fn the_attempt_bound_survives_a_restart() {
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;
    say(&mut w.admin, &mut w.admin_stream, b"m1").await;
    pings_settle(&w.server, &w.phone, 1).await;

    let mut expected = 1usize;
    for _ in 0..CATCH_UP_MAX {
        w.server.clock.advance(MINUTE);
        w.server.sweep().await;
        expected += 1;
        pings_settle(&w.server, &w.phone, expected).await;
    }

    // Restart with the same database, clock and push recorder. The attempt
    // count is durable, so a fresh server does not resume catch-up pings even
    // though its in-memory rate limiter is reset.
    w.server.restart().await;
    w.server.clock.advance(MINUTE);
    w.server.sweep().await;
    pings_settle(&w.server, &w.phone, expected).await;
}
