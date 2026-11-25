//! wire-transport: Single persistent messaging stream; Heartbeats; Protocol
//! version negotiation; Size limits (the server's side); device-identity: Device
//! revocation (its open stream closes). All over `Messaging.Connect`.

mod support;

use std::time::Duration;

use protocol::wire_transport::{PROTOCOL_VERSION, SESSION_TOKEN_MAX_LIFETIME};
use support::Check;
use support::harness::authed;
use support::messaging::{Stream, commit_envelope, envelope, wait_offline, world};
use support::pb;
use support::pb::client_frame::Frame as In;
use support::pb::envelope::Kind;
use support::pb::server_frame::Frame as Out;
use tonic::Code;

/// The last `server_seq` handed out: unchanged means nothing was stored.
async fn envelope_count(server: &support::harness::TestServer) -> i64 {
    server
        .query(|db| db.query_row("SELECT last_server_seq FROM sequence", [], |r| r.get(0)))
        .await
}

// spec: wire-transport/single-persistent-messaging-stream
#[tokio::test]
async fn second_stream_from_the_same_device_aborts_the_first() {
    let mut w = world().await;
    let mut second = Stream::open(&w.conn, &w.phone).await;
    let ended = w.phone_stream.ended().await;
    assert_eq!(ended.err().map(|s| s.code()), Some(Code::Aborted));

    // The new stream carries the device's traffic.
    let epoch = w.admin.epoch();
    let text = w.admin.message(b"hello Bob");
    let seq = w
        .admin_stream
        .accepted(envelope(Kind::Application, epoch, text))
        .await;
    let got = second.envelope().await;
    assert_eq!(got.server_seq, seq);
    assert_eq!(w.phone.read_message(&got.ciphertext), b"hello Bob");
}

// spec: wire-transport/single-persistent-messaging-stream
#[tokio::test]
async fn one_stream_carries_sends_results_deliveries_and_acks() {
    let mut w = world().await;
    let epoch = w.laptop.epoch();
    let text = w.laptop.message(b"hi all");
    let seq = w
        .laptop_stream
        .accepted(envelope(Kind::Application, epoch, text))
        .await;
    for (device, stream) in [
        (&mut w.admin, &mut w.admin_stream),
        (&mut w.phone, &mut w.phone_stream),
    ] {
        let got = stream.envelope().await;
        assert_eq!(got.server_seq, seq);
        assert_eq!(device.read_message(&got.ciphertext), b"hi all");
        stream.ack(seq).await;
    }
    // The sender does not get its own message back.
    w.laptop_stream.assert_quiet().await;
}

// spec: wire-transport/heartbeats
#[tokio::test]
async fn server_replies_to_each_heartbeat() {
    let mut w = world().await;
    for _ in 0..3 {
        w.phone_stream.send(In::Heartbeat(pb::Heartbeat {})).await;
        assert!(matches!(w.phone_stream.next().await, Out::Heartbeat(_)));
    }
}

// spec: wire-transport/heartbeats
#[tokio::test]
async fn silent_client_is_closed_after_90_seconds_and_treated_as_offline() {
    let mut w = world().await;
    // Activity within 90 seconds keeps the stream open.
    w.server.clock.advance(Duration::from_secs(60));
    w.phone_stream.send(In::Heartbeat(pb::Heartbeat {})).await;
    assert!(matches!(w.phone_stream.next().await, Out::Heartbeat(_)));
    w.server.clock.advance(Duration::from_secs(60));
    tokio::time::sleep(Duration::from_millis(1500)).await;
    w.phone_stream.send(In::Heartbeat(pb::Heartbeat {})).await;
    assert!(matches!(w.phone_stream.next().await, Out::Heartbeat(_)));

    // Nothing for 90 seconds: the server closes the stream.
    w.server.clock.advance(Duration::from_secs(90));
    let ended = w.phone_stream.ended().await;
    assert_eq!(ended.err().map(|s| s.code()), Some(Code::Unavailable));
    // The server unregisters the stream just after ending it.
    wait_offline(&w.server, &w.phone).await;
}

// spec: wire-transport/protocol-version-negotiation
#[tokio::test]
async fn unsupported_version_is_failed_precondition_listing_supported_versions() {
    let w = world().await;
    let hello = pb::ClientFrame {
        frame: Some(In::Hello(pb::Hello {
            protocol_version: 99,
        })),
    };
    let mut stream = Stream::open_with(&w.conn, w.phone.token(), Some(hello))
        .await
        .check("Connect");
    let status = stream.ended().await.err().check("an error status");
    assert_eq!(status.code(), Code::FailedPrecondition);
    assert!(
        status.message().contains("supported versions: [1, 2, 3, 4]"),
        "{}",
        status.message()
    );
}

// spec: wire-transport/protocol-version-negotiation
#[tokio::test]
async fn an_old_app_says_version_1_and_is_served() {
    let w = world().await;
    for version in [1, 2] {
        let hello = pb::ClientFrame {
            frame: Some(In::Hello(pb::Hello {
                protocol_version: version,
            })),
        };
        let mut stream = Stream::open_with(&w.conn, w.phone.token(), Some(hello))
            .await
            .check("Connect");
        stream.send(In::Heartbeat(pb::Heartbeat {})).await;
        assert!(
            matches!(stream.next().await, Out::Heartbeat(_)),
            "version {version}"
        );
    }
}

// spec: wire-transport/protocol-version-negotiation
#[tokio::test]
async fn first_frame_must_be_hello_and_only_the_first() {
    let mut w = world().await;
    let heartbeat = pb::ClientFrame {
        frame: Some(In::Heartbeat(pb::Heartbeat {})),
    };
    let mut stream = Stream::open_with(&w.conn, w.phone.token(), Some(heartbeat))
        .await
        .check("Connect");
    assert_eq!(
        stream.ended().await.err().map(|s| s.code()),
        Some(Code::InvalidArgument)
    );

    w.laptop_stream
        .send(In::Hello(pb::Hello {
            protocol_version: 1,
        }))
        .await;
    assert_eq!(
        w.laptop_stream.ended().await.err().map(|s| s.code()),
        Some(Code::InvalidArgument)
    );
}

// spec: wire-transport/session-authentication
#[tokio::test]
async fn connect_needs_a_session_token() {
    let w = world().await;
    let (_tx, frames) = tokio::sync::mpsc::channel::<pb::ClientFrame>(1);
    let result = w
        .conn
        .messaging()
        .connect(tokio_stream::wrappers::ReceiverStream::new(frames))
        .await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::Unauthenticated));
    let (_tx, frames) = tokio::sync::mpsc::channel::<pb::ClientFrame>(1);
    let result = w
        .conn
        .messaging()
        .connect(authed(
            "not-a-token",
            tokio_stream::wrappers::ReceiverStream::new(frames),
        ))
        .await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::Unauthenticated));
}

// spec: wire-transport/size-limits
#[tokio::test]
async fn oversized_application_message_is_too_large_and_stores_nothing() {
    let mut w = world().await;
    let before = envelope_count(&w.server).await;
    let epoch = w.admin.epoch();
    let rejected = w
        .admin_stream
        .rejected(envelope(Kind::Application, epoch, vec![0u8; 65_537]))
        .await;
    assert_eq!(rejected.reason, pb::RejectionReason::TooLarge as i32);
    assert_eq!(envelope_count(&w.server).await, before);
    w.laptop_stream.assert_quiet().await;
}

// spec: wire-transport/size-limits
#[tokio::test]
async fn welcome_counts_toward_the_commit_cap() {
    let mut w = world().await;
    let before = envelope_count(&w.server).await;
    let epoch = w.admin.epoch();
    let carol = support::clients::TestUser::new("carol");
    let carol_phone = carol.new_device();
    // A 200,000-byte commit with a 70,000-byte Welcome is over 262,144 bytes.
    let big = commit_envelope(
        epoch,
        vec![0u8; 200_000],
        Some((vec![0u8; 70_000], &[&carol_phone])),
    );
    let rejected = w.admin_stream.rejected(big).await;
    assert_eq!(rejected.reason, pb::RejectionReason::TooLarge as i32);
    // The same commit alone is under the cap (and then fails as malformed).
    let alone = commit_envelope(epoch, vec![0u8; 200_000], None);
    let rejected = w.admin_stream.rejected(alone).await;
    assert_eq!(rejected.reason, pb::RejectionReason::Malformed as i32);
    let proposal = envelope(Kind::Proposal, epoch, vec![0u8; 262_145]);
    let rejected = w.admin_stream.rejected(proposal).await;
    assert_eq!(rejected.reason, pb::RejectionReason::TooLarge as i32);
    assert_eq!(envelope_count(&w.server).await, before);
}

// spec: device-identity/device-revocation
#[tokio::test]
async fn revoked_devices_open_stream_closes() {
    let mut w = world().await;
    let mut tablet = support::link_device(&w.conn, &w.alice, &w.laptop).await;
    let mut tablet_stream = Stream::open(&w.conn, &tablet).await;
    // Let the stream register before revoking.
    tablet_stream.send(In::Heartbeat(pb::Heartbeat {})).await;
    assert!(matches!(tablet_stream.next().await, Out::LowKeyPackages(_)));
    assert!(matches!(tablet_stream.next().await, Out::Heartbeat(_)));
    w.conn
        .devices()
        .revoke_device(authed(
            w.laptop.token(),
            pb::RevokeDeviceRequest {
                device_id: tablet.device_id.to_vec(),
            },
        ))
        .await
        .check("RevokeDevice");
    assert_eq!(
        tablet_stream.ended().await.err().map(|s| s.code()),
        Some(Code::PermissionDenied)
    );
    tablet.token = None;

    // RemoveUser closes every stream of the user.
    w.conn
        .admin()
        .remove_user(authed(
            w.admin.token(),
            pb::RemoveUserRequest {
                user_id: Some(w.bob.pb_user_id()),
            },
        ))
        .await
        .check("RemoveUser");
    assert_eq!(
        w.phone_stream.ended().await.err().map(|s| s.code()),
        Some(Code::PermissionDenied)
    );
}

// A session token is not tied to the connection that minted it: the device
// core reuses it to open the next `Connect` stream after a reconnect without a
// fresh challenge (wire-transport: Session resume).

// spec: wire-transport/session-resume
#[tokio::test]
async fn a_token_minted_on_one_connection_opens_connect_on_another() {
    let mut w = world().await;
    // A fresh TLS connection, as a reconnect makes; its exporter differs.
    let second = w.server.connect().await;
    assert_ne!(w.conn.exporter, second.exporter);

    // The phone's token was obtained on `w.conn`. Opening the phone's stream on
    // `second` with that same token is accepted.
    let mut stream = Stream::open(&second, &w.phone).await;
    // One stream per device: the first is aborted, the new one serves.
    assert_eq!(
        w.phone_stream.ended().await.err().map(|s| s.code()),
        Some(Code::Aborted)
    );
    stream.send(In::Heartbeat(pb::Heartbeat {})).await;
    assert!(matches!(stream.next().await, Out::Heartbeat(_)));
}

// spec: wire-transport/session-resume
#[tokio::test]
async fn connect_with_an_expired_token_is_unauthenticated() {
    let w = world().await;
    // The hour-long token ages out.
    w.server
        .clock
        .advance(SESSION_TOKEN_MAX_LIFETIME + Duration::from_secs(1));
    let hello = pb::ClientFrame {
        frame: Some(In::Hello(pb::Hello {
            protocol_version: PROTOCOL_VERSION,
        })),
    };
    // The server refuses the stream, whether at the open or as its first status.
    match Stream::open_with(&w.conn, w.phone.token(), Some(hello)).await {
        Err(status) => assert_eq!(status.code(), Code::Unauthenticated),
        Ok(mut stream) => {
            let status = stream.ended().await.err().check("an error status");
            assert_eq!(status.code(), Code::Unauthenticated);
        }
    }
}
