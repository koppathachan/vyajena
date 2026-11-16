//! device-identity: Push token registration (token limits, revoked devices
//! lose their token) and pi-runtime: Push errors (a token the platform calls
//! invalid is cleared, unless a new one raced it).

mod support;

use std::time::Duration;

use protocol::device_identity::DeviceId;
use support::clients::TestDevice;
use support::harness::{Conn, TestPi, authed};
use support::messaging::{offline, say, wait_offline, world};
use support::{Check, pb};
use tonic::Code;

async fn update(
    conn: &Conn,
    device: &TestDevice,
    platform: pb::PushPlatform,
    token: &str,
) -> Result<(), tonic::Status> {
    conn.devices()
        .update_push_token(authed(
            device.token(),
            pb::UpdatePushTokenRequest {
                push_token: Some(pb::PushToken {
                    platform: platform as i32,
                    token: token.to_owned(),
                }),
            },
        ))
        .await
        .map(|_| ())
}

async fn stored(pi: &TestPi, device_id: DeviceId) -> Option<String> {
    pi.query(move |conn| {
        conn.query_row(
            "SELECT push_token FROM devices WHERE device_id = ?1",
            [device_id.as_slice()],
            |row| row.get(0),
        )
    })
    .await
}

// spec: device-identity/push-token-registration
#[tokio::test]
async fn the_pi_refuses_oversized_spaced_and_non_hex_apns_tokens() {
    let w = world().await;
    let apns = "ab".repeat(32);
    update(&w.conn, &w.phone, pb::PushPlatform::Apns, &apns)
        .await
        .check("a 64-hex APNs token");
    let refused = [
        (pb::PushPlatform::Apns, "../../3/device/x".to_owned()),
        (pb::PushPlatform::Apns, "ab".repeat(31)),
        (pb::PushPlatform::Apns, "ab".repeat(101)),
        (pb::PushPlatform::Apns, format!("{}zz", "ab".repeat(32))),
        (pb::PushPlatform::Fcm, "x".repeat(4097)),
        (pb::PushPlatform::Fcm, "has space".to_owned()),
        (pb::PushPlatform::Fcm, "line\nbreak".to_owned()),
        (pb::PushPlatform::Fcm, String::new()),
    ];
    for (platform, token) in refused {
        let result = update(&w.conn, &w.phone, platform, &token).await;
        assert_eq!(
            result.err().map(|s| s.code()),
            Some(Code::InvalidArgument),
            "{platform:?} {:?}",
            &token[..token.len().min(20)]
        );
    }
    // The old token stays.
    assert_eq!(stored(&w.pi, w.phone.device_id).await, Some(apns));
    // 4096 bytes is fine for FCM.
    update(&w.conn, &w.phone, pb::PushPlatform::Fcm, &"x".repeat(4096))
        .await
        .check("a 4096-byte FCM token");
}

// spec: device-identity/push-token-registration
#[tokio::test]
async fn a_revoked_device_loses_its_token_and_is_never_pinged() {
    let mut w = world().await;
    update(&w.conn, &w.phone, pb::PushPlatform::Fcm, "phone-token")
        .await
        .check("token");
    // The admin removes Bob: his phone is revoked.
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
    assert_eq!(stored(&w.pi, w.phone.device_id).await, None);
    say(&mut w.admin, &mut w.admin_stream, b"after").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(w.pi.pushes.count(&w.phone.device_id), 0);
}

// spec: pi-runtime/push-errors
#[tokio::test]
async fn a_token_the_platform_calls_invalid_is_cleared() {
    let mut w = world().await;
    update(&w.conn, &w.phone, pb::PushPlatform::Fcm, "gone-token")
        .await
        .check("token");
    w.pi.pushes.answer_invalid("gone-token");
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.pi, &w.phone).await;
    say(&mut w.admin, &mut w.admin_stream, b"hello").await;

    let deadline = tokio::time::Instant::now() + support::messaging::WAIT;
    while stored(&w.pi, w.phone.device_id).await.is_some() {
        assert!(tokio::time::Instant::now() < deadline, "token not cleared");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(w.pi.pushes.count(&w.phone.device_id), 1);
    // No token, no more pings (even after the 60 s limit).
    w.pi.clock.advance(Duration::from_secs(61));
    say(&mut w.admin, &mut w.admin_stream, b"again").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(w.pi.pushes.count(&w.phone.device_id), 1);
}

// spec: pi-runtime/push-errors
#[tokio::test]
async fn a_new_token_that_raced_the_error_is_kept() {
    let w = world().await;
    update(&w.conn, &w.phone, pb::PushPlatform::Fcm, "new-token")
        .await
        .check("token");
    // The platform's "invalid" answer was for the old token.
    let device_id = w.phone.device_id;
    let cleared = w
        .pi
        .state
        .store
        .call(move |conn| server::store::records::clear_push_token(conn, &device_id, "old-token"))
        .await
        .check("clear");
    assert!(!cleared);
    assert_eq!(
        stored(&w.pi, w.phone.device_id).await.as_deref(),
        Some("new-token")
    );
}
