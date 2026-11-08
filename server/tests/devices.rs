//! device-identity: Device linking; Device revocation; Push token
//! registration; Admin awareness (Devices.LinkDevice, GetSelf, RevokeDevice,
//! UpdatePushToken).

mod support;

use support::clients::TestDevice;
use support::harness::{TestPi, authed};
use support::{Check, bootstrap_admin, invite_user, link_device, pb};
use tonic::Code;

// spec: device-identity/device-linking
#[tokio::test]
async fn linked_phone_authenticates_and_gets_its_certificate() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let (alice, laptop) = invite_user(&conn, &admin, "alice").await;

    let phone = link_device(&conn, &alice, &laptop).await;
    let me = conn
        .devices()
        .get_self(authed(phone.token(), pb::GetSelfRequest {}))
        .await
        .check("GetSelf")
        .into_inner();
    assert_eq!(me.user_id, Some(alice.pb_user_id()));
    assert_eq!(me.certificate, Some(phone.certificate.clone()));
}

// spec: device-identity/device-linking
#[tokio::test]
async fn linking_without_the_identity_key_is_rejected() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let (_alice, laptop) = invite_user(&conn, &admin, "alice").await;

    // A certificate for alice@pi signed by a key that is not Alice's identity.
    let impostor = TestDevice::new("alice", &support::clients::new_key());
    let result = conn
        .devices()
        .link_device(authed(
            laptop.token(),
            pb::LinkDeviceRequest {
                certificate: Some(impostor.certificate.clone()),
            },
        ))
        .await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::PermissionDenied));
    let auth = impostor.try_authenticate_with(&conn, &conn.exporter).await;
    assert_eq!(auth.err().map(|s| s.code()), Some(Code::Unauthenticated));
}

// spec: device-identity/device-linking
#[tokio::test]
async fn linking_a_device_for_another_user_is_rejected() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let (_alice, laptop) = invite_user(&conn, &admin, "alice").await;
    let (bob, _) = invite_user(&conn, &admin, "bob").await;
    // A genuine certificate of Bob's, uploaded by Alice's device.
    let bobs = bob.new_device();
    let result = conn
        .devices()
        .link_device(authed(
            laptop.token(),
            pb::LinkDeviceRequest {
                certificate: Some(bobs.certificate.clone()),
            },
        ))
        .await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::PermissionDenied));
}

// spec: device-identity/device-linking
#[tokio::test]
async fn linking_the_same_device_twice_is_already_exists() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let (alice, laptop) = invite_user(&conn, &admin, "alice").await;
    let phone = link_device(&conn, &alice, &laptop).await;
    let again = conn
        .devices()
        .link_device(authed(
            laptop.token(),
            pb::LinkDeviceRequest {
                certificate: Some(phone.certificate.clone()),
            },
        ))
        .await;
    assert_eq!(again.err().map(|s| s.code()), Some(Code::AlreadyExists));
}

// spec: device-identity/device-revocation
#[tokio::test]
async fn lost_phone_is_revoked_from_the_tablet() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let (alice, tablet) = invite_user(&conn, &admin, "alice").await;
    let phone = link_device(&conn, &alice, &tablet).await;
    let mut revocations = pi.state.revocations.subscribe();

    conn.devices()
        .revoke_device(authed(
            tablet.token(),
            pb::RevokeDeviceRequest {
                device_id: phone.device_id.to_vec(),
            },
        ))
        .await
        .check("RevokeDevice");

    // Its session ends now and it cannot authenticate again.
    let old_session = conn
        .devices()
        .get_self(authed(phone.token(), pb::GetSelfRequest {}))
        .await;
    assert_eq!(
        old_session.err().map(|s| s.code()),
        Some(Code::Unauthenticated)
    );
    let auth = phone.try_authenticate_with(&conn, &conn.exporter).await;
    assert_eq!(auth.err().map(|s| s.code()), Some(Code::Unauthenticated));
    // The messaging service is told, to close its open stream.
    assert_eq!(revocations.try_recv().ok(), Some(phone.device_id));
    // The tablet is unaffected.
    conn.devices()
        .get_self(authed(tablet.token(), pb::GetSelfRequest {}))
        .await
        .check("tablet still works");
}

// spec: device-identity/device-revocation
#[tokio::test]
async fn devices_of_other_users_cannot_be_revoked() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let (_, alice) = invite_user(&conn, &admin, "alice").await;
    let (_, bob) = invite_user(&conn, &admin, "bob").await;
    let result = conn
        .devices()
        .revoke_device(authed(
            alice.token(),
            pb::RevokeDeviceRequest {
                device_id: bob.device_id.to_vec(),
            },
        ))
        .await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::NotFound));
    conn.devices()
        .get_self(authed(bob.token(), pb::GetSelfRequest {}))
        .await
        .check("Bob is unaffected");
}

async fn stored_push_token(pi: &TestPi, device: &TestDevice) -> Option<pb::PushToken> {
    let id = device.device_id;
    pi.state
        .store
        .call(move |db| server::store::records::push_token(db, &id))
        .await
        .check("push token")
        .map(|t| pb::PushToken {
            platform: t.platform,
            token: t.token,
        })
}

// spec: device-identity/push-token-registration
#[tokio::test]
async fn new_fcm_token_replaces_the_old_one_and_can_be_cleared() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let token = admin.token().to_owned();
    let update = |push_token: Option<pb::PushToken>| {
        let mut client = conn.devices();
        let request = authed(&token, pb::UpdatePushTokenRequest { push_token });
        async move { client.update_push_token(request).await }
    };
    let fcm = |token: &str| pb::PushToken {
        platform: pb::PushPlatform::Fcm as i32,
        token: token.to_owned(),
    };

    update(Some(fcm("first"))).await.check("set");
    assert_eq!(stored_push_token(&pi, &admin).await, Some(fcm("first")));
    update(Some(fcm("second"))).await.check("replace");
    assert_eq!(stored_push_token(&pi, &admin).await, Some(fcm("second")));
    let apns = pb::PushToken {
        platform: pb::PushPlatform::Apns as i32,
        token: "apns".to_owned(),
    };
    update(Some(apns.clone())).await.check("APNs");
    assert_eq!(stored_push_token(&pi, &admin).await, Some(apns));
    update(None).await.check("clear");
    assert_eq!(stored_push_token(&pi, &admin).await, None);

    let unspecified = update(Some(pb::PushToken {
        platform: pb::PushPlatform::Unspecified as i32,
        token: "x".to_owned(),
    }))
    .await;
    assert_eq!(
        unspecified.err().map(|s| s.code()),
        Some(Code::InvalidArgument)
    );
}

// spec: device-identity/admin-awareness
#[tokio::test]
async fn admins_first_device_is_told_it_is_the_admin() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (admin, device) = bootstrap_admin(&pi, &conn).await;
    let me = conn
        .devices()
        .get_self(authed(device.token(), pb::GetSelfRequest {}))
        .await
        .check("GetSelf")
        .into_inner();
    assert!(me.is_admin);
    assert_eq!(me.user_id, Some(admin.pb_user_id()));
    assert_eq!(me.certificate, Some(device.certificate.clone()));
}

// spec: device-identity/admin-awareness
#[tokio::test]
async fn ordinary_member_is_not_the_admin() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let (_, alice) = invite_user(&conn, &admin, "alice").await;
    let me = conn
        .devices()
        .get_self(authed(alice.token(), pb::GetSelfRequest {}))
        .await
        .check("GetSelf")
        .into_inner();
    assert!(!me.is_admin);
}
