//! client-runtime: Connection lifecycle, end to end against the real Pi:
//! pinned TLS, challenge auth, token renewal, one `Connect` stream and
//! reconnect with backoff (client-core task 4.2).

mod support;

use std::time::Duration;

use client::identity::Invite;
use client::net::NetError;
use client::{ClientError, ConnectionState, Event};
use support::Check;
use support::device::{self, Device, member, user};
use support::pi::TestPi;

// spec: client-runtime/connection-lifecycle
// spec: wire-transport/grpc-over-http-2-and-pinned-tls-on-tcp-443
#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_pin_is_a_server_identity_error() {
    let pi = TestPi::start().await;
    let mut invite = Invite::decode(&pi.bootstrap).check("bootstrap invite");
    invite.tls_pin[0] ^= 0xff;
    let mut admin = Device::new().await;
    let result = admin.client.register(&invite.encode()).await;
    assert!(
        matches!(result, Err(ClientError::Net(NetError::ServerIdentity))),
        "{result:?}"
    );
    admin
        .wait_event("a server identity error", |e| {
            matches!(e, Event::ServerIdentityError)
        })
        .await;
    // Nothing was created on the Pi: the right invite still works.
    admin.client.register(&pi.bootstrap).await.check("register");
}

// spec: client-runtime/connection-lifecycle
// spec: wire-transport/single-persistent-messaging-stream
#[tokio::test(flavor = "multi_thread")]
async fn the_core_reconnects_on_its_own_and_catches_up_in_order() {
    let mut pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    let mut alice = device::member_of_group(&admin, "alice").await;
    assert!(pi.is_connected(&alice.device_id()));

    // The network goes away (as when Alice's laptop loses it for a minute),
    // and the Pi restarts meanwhile.
    pi.relay.cut();
    alice
        .wait_event("the drop", |e| {
            matches!(e, Event::Connection(ConnectionState::Connecting))
        })
        .await;
    pi.restart().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    pi.relay.restore();
    alice.wait_connected().await;
    let mut admin = admin;
    admin.wait_connected().await;

    // While Alice is away again, the admin adds Bob; she catches up.
    alice.client.stop();
    let mut bob = device::invited(&admin, "bob").await;
    admin.client.add_user(&user("bob")).await.check("add bob");
    bob.wait_event("bob joined", |e| matches!(e, Event::JoinedGroup))
        .await;
    alice.client.start().await.check("start alice");
    alice.wait_connected().await;
    alice
        .wait_members(&[member(&admin), member(&alice), member(&bob)])
        .await;
    assert_eq!(alice.epoch(), admin.epoch());
}

// spec: client-runtime/connection-lifecycle
// spec: wire-transport/session-authentication
#[tokio::test(flavor = "multi_thread")]
async fn the_session_token_is_renewed_before_it_expires() {
    let pi = TestPi::start().await;
    let mut admin = device::admin(&pi).await;
    let id = admin.device_id();
    let sessions = move |pi: &TestPi| {
        let state = pi.state.clone();
        async move {
            state
                .store
                .call(move |conn| {
                    Ok(conn.query_row(
                        "SELECT count(*) FROM sessions WHERE device_id = ?1",
                        [id.as_slice()],
                        |row| row.get::<_, i64>(0),
                    )?)
                })
                .await
                .check("sessions")
        }
    };
    let before = sessions(&pi).await;

    // 56 minutes later on the device's clock the hour-long token is close to
    // expiry: the core authenticates again on the same connection.
    admin.clock.advance(Duration::from_secs(56 * 60));
    let deadline = tokio::time::Instant::now() + support::device::WAIT;
    while sessions(&pi).await <= before {
        assert!(tokio::time::Instant::now() < deadline, "no renewal");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // The stream stayed up, and calls work with the new token.
    assert!(
        !admin
            .sees_within_quiet(|e| matches!(e, Event::Connection(ConnectionState::Connecting)))
            .await
    );
    assert!(pi.is_connected(&id));
    admin.client.create_invite("alice").await.check("invite");
}

// spec: client-runtime/connection-lifecycle
#[tokio::test(flavor = "multi_thread")]
async fn a_device_clock_ahead_of_the_pi_does_not_renew_on_every_tick() {
    let pi = TestPi::start().await;
    let admin = device::admin(&pi).await;
    let id = admin.device_id();
    let count = move |pi: &TestPi| {
        let state = pi.state.clone();
        async move {
            state
                .store
                .call(move |conn| {
                    Ok(conn.query_row(
                        "SELECT count(*) FROM sessions WHERE device_id = ?1",
                        [id.as_slice()],
                        |row| row.get::<_, i64>(0),
                    )?)
                })
                .await
                .check("sessions")
        }
    };
    // Three days ahead: every token the Pi issues looks expired already.
    admin.clock.advance(Duration::from_secs(3 * 24 * 60 * 60));
    tokio::time::sleep(Duration::from_millis(300)).await;
    let after_first = count(&pi).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(count(&pi).await, after_first, "renewing in a loop");
    assert!(pi.is_connected(&id));
}

// spec: client-runtime/connection-lifecycle
#[tokio::test(flavor = "multi_thread")]
async fn a_registered_device_refuses_a_server_with_another_key() {
    let mut pi = TestPi::start().await;
    let mut admin = device::admin(&pi).await;
    pi.relay.cut();
    pi.restart_with_new_keys().await;
    pi.relay.restore();
    admin
        .wait_event("a server identity error", |e| {
            matches!(e, Event::ServerIdentityError)
        })
        .await;
    // It stops trying until it is started again.
    assert!(!admin.client.is_connected());
    assert!(
        !admin
            .sees_within_quiet(|e| matches!(e, Event::Connection(ConnectionState::Connected)))
            .await
    );
    assert!(!pi.is_connected(&admin.device_id()));
}

// spec: wire-transport/heartbeats
// spec: wire-transport/protocol-version-negotiation
// spec: client-runtime/connection-lifecycle
#[tokio::test(flavor = "multi_thread")]
async fn heartbeats_keep_an_idle_stream_open() {
    let pi = TestPi::start().await;
    // The Pi accepted the stream, so the first frame named a protocol
    // version it supports.
    let mut admin = device::admin(&pi).await;
    let id = admin.device_id();
    assert!(pi.is_connected(&id));

    // 80 s pass on the Pi's clock with the stream idle, then the core's
    // heartbeat (sent after 25 s of real idle time) arrives. 20 s later the
    // stream has been open 100 s, more than the Pi's 90 s idle limit, but
    // silent for only 20 s: the Pi keeps it.
    pi.clock.advance(Duration::from_secs(80));
    tokio::time::sleep(protocol::wire_transport::HEARTBEAT_INTERVAL + Duration::from_secs(2)).await;
    pi.clock.advance(Duration::from_secs(20));
    assert!(
        !admin
            .sees_within_quiet(|e| matches!(e, Event::Connection(ConnectionState::Connecting)))
            .await,
        "{:?}",
        admin.events()
    );
    // The Pi checks idle streams every second.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(pi.is_connected(&id));
    assert!(
        !admin
            .sees_within_quiet(|e| matches!(e, Event::Connection(ConnectionState::Connecting)))
            .await,
        "{:?}",
        admin.events()
    );
}

// spec: client-runtime/connection-lifecycle
#[tokio::test(flavor = "multi_thread")]
async fn a_removed_device_is_told_once_and_stops_connecting() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    let sent = bob.client.send_text("before the removal").check("send");
    bob.wait_event("accepted", |e| {
        matches!(e, Event::SendStatus { id, status: client::SendStatus::Accepted } if *id == sent)
    })
    .await;

    // The Pi closes Bob's open stream with PERMISSION_DENIED.
    admin
        .client
        .remove_user(&user("bob"))
        .await
        .check("remove bob");
    bob.wait_event("removed", |e| matches!(e, Event::Removed))
        .await;
    bob.wait_event("disconnected", |e| {
        matches!(e, Event::Connection(ConnectionState::Disconnected))
    })
    .await;
    // No retry: no new connecting state, no second Removed.
    assert!(
        !bob.sees_within_quiet(|e| matches!(
            e,
            Event::Connection(ConnectionState::Connecting | ConnectionState::Connected)
                | Event::Removed
        ))
        .await,
        "{:?}",
        bob.events()
    );
    assert!(!bob.client.is_connected());
    assert!(!pi.is_connected(&bob.device_id()));
    // reconnect_now does not undo it.
    bob.client.reconnect_now();
    assert!(
        !bob.sees_within_quiet(|e| matches!(e, Event::Connection(ConnectionState::Connecting)))
            .await
    );
    // History stays readable.
    let history = bob.client.history(10).check("history");
    assert!(history.iter().any(|m| m.id == sent), "{history:?}");
}

// spec: client-runtime/connection-lifecycle
#[tokio::test(flavor = "multi_thread")]
async fn a_revoked_device_is_told_by_authenticate() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    bob.client.stop();
    admin
        .client
        .remove_user(&user("bob"))
        .await
        .check("remove bob");

    // Back online: Devices.Authenticate answers PERMISSION_DENIED.
    bob.client.start().await.check("start bob");
    bob.wait_event("removed", |e| matches!(e, Event::Removed))
        .await;
    assert!(
        !bob.sees_within_quiet(|e| matches!(
            e,
            Event::Connection(ConnectionState::Connecting | ConnectionState::Connected)
                | Event::Removed
        ))
        .await,
        "{:?}",
        bob.events()
    );
    assert!(!pi.is_connected(&bob.device_id()));
}

// spec: client-runtime/connection-lifecycle
#[tokio::test(flavor = "multi_thread")]
async fn reconnect_now_skips_the_backoff_wait() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    let mut alice = device::member_of_group(&admin, "alice").await;

    // While connected it does nothing.
    alice.client.reconnect_now();
    assert!(
        !alice
            .sees_within_quiet(|e| matches!(e, Event::Connection(ConnectionState::Connecting)))
            .await
    );
    assert!(alice.client.is_connected());

    // The network goes away. After the drop and four failed attempts the
    // next wait is at least 4 seconds (backoff 0.25, 0.5, 1, 2, then 4).
    pi.relay.cut();
    for what in [
        "the drop",
        "attempt 1",
        "attempt 2",
        "attempt 3",
        "attempt 4",
    ] {
        alice
            .wait_event(what, |e| {
                matches!(e, Event::Connection(ConnectionState::Connecting))
            })
            .await;
    }
    // Let attempt 4 fail (the relay refuses at once).
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    pi.relay.restore();
    let asked = std::time::Instant::now();
    alice.client.reconnect_now();
    alice.wait_connected().await;
    assert!(
        asked.elapsed() < std::time::Duration::from_secs(3),
        "connected after {:?}",
        asked.elapsed()
    );
}
