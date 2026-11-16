//! relay: TLS ends on the Pi, and client-runtime: Connection lifecycle
//! (relay without a tunnel), through the test relay that stands in for the
//! GCP relay (relay change, design D4, D11).

mod support;

use std::time::Duration;

use client::identity::Invite;
use client::{ConnectionState, Event, MessageBody};
use support::device::{self, Device};
use support::pi::TestPi;
use support::{Check, contains};

fn is_text(event: &Event, text: &str) -> bool {
    matches!(event, Event::Message(m) if m.body == MessageBody::Text(text.to_owned()))
}

async fn wait_text(device: &mut Device, text: &str) {
    device
        .wait_event(&format!("the message {text:?}"), |e| is_text(e, text))
        .await;
}

// spec: relay/tls-ends-on-the-pi
#[tokio::test(flavor = "multi_thread")]
async fn the_relay_forwards_tls_bytes_and_never_sees_the_text() {
    let pi = TestPi::start().await;
    let mut admin = device::admin_with_group(&pi).await;
    let mut bob = device::member_of_group(&admin, "bob").await;

    pi.relay.record();
    let text = "hi Bob, the relay must not read this";
    admin.client.send_text(text).check("send");
    wait_text(&mut bob, text).await;
    bob.client.send_text("hi admin, nor this").check("send");
    wait_text(&mut admin, "hi admin, nor this").await;

    let seen = pi.relay.recorded();
    assert!(seen.len() > 200, "the relay forwarded {} bytes", seen.len());
    for secret in [text, "hi admin, nor this", "the relay must not"] {
        assert!(
            !contains(&seen, secret.as_bytes()),
            "the relay saw {secret:?}"
        );
    }
    // TLS records only: every forwarded byte stream is TLS application data,
    // which starts with content type 23 (0x17) after the handshake.
    assert!(
        contains(&seen, &[0x17, 0x03, 0x03]),
        "no TLS 1.3 records seen"
    );
}

// spec: relay/relay-host
#[tokio::test(flavor = "multi_thread")]
async fn an_invite_naming_the_relay_by_dns_name_connects() {
    // Invites name vyajarelay.purplechaamba.com:443; here "localhost" stands in for
    // the name: the core resolves it and still checks only the Pi's pin.
    let pi = TestPi::start().await;
    let mut invite = Invite::decode(&pi.bootstrap).check("bootstrap invite");
    invite.relay_address = format!("localhost:{}", pi.relay.addr.port());
    let mut admin = Device::new().await;
    admin
        .client
        .register(&invite.encode())
        .await
        .check("register through the named relay");
    admin.client.start().await.check("start");
    admin.wait_connected().await;
}

// spec: relay/tls-ends-on-the-pi
#[tokio::test(flavor = "multi_thread")]
async fn a_relay_forwarding_to_another_server_is_refused() {
    let pi = TestPi::start().await;
    let mut admin = device::admin(&pi).await;
    // The relay now forwards to another server, with its own TLS key, as a
    // relay that answered TLS itself would.
    let impostor = TestPi::start().await;
    pi.relay.cut();
    pi.relay.retarget(impostor.addr);
    pi.relay.restore();
    admin
        .wait_event("a server identity error", |e| {
            matches!(e, Event::ServerIdentityError)
        })
        .await;
    assert!(!admin.client.is_connected());
    assert!(!impostor.is_connected(&admin.device_id()));
}

// spec: client-runtime/connection-lifecycle
#[tokio::test(flavor = "multi_thread")]
async fn without_a_tunnel_the_core_retries_and_catches_up_when_it_is_back() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    let mut bob = device::member_of_group(&admin, "bob").await;

    // The Pi's tunnel goes down: the relay accepts connections on 443 and
    // closes them at once (the real relay does this until its heartbeat drops
    // the tunnel, then refuses).
    pi.relay.cut();
    bob.wait_event("the drop", |e| {
        matches!(e, Event::Connection(ConnectionState::Connecting))
    })
    .await;
    // The admin sends meanwhile; the core keeps it as sending.
    let text = "sent while the tunnel was down";
    admin.client.send_text(text).check("send");

    // The cores keep retrying with backoff (0.25 s, then 0.5 s, ...).
    let accepted = pi.relay.accepted();
    let deadline = tokio::time::Instant::now() + support::device::WAIT;
    while pi.relay.accepted() - accepted < 2 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the cores stopped retrying ({} attempts)",
            pi.relay.accepted() - accepted
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!bob.client.is_connected());

    // The tunnel is back: same relay address, same pin, the message arrives.
    pi.relay.restore();
    bob.wait_connected().await;
    wait_text(&mut bob, text).await;
}
