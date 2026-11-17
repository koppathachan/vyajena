//! device-identity: Signup with an invite; Admin-owned usernames; User identity
//! key and device certificates; Federation-ready identifiers; Bootstrap admin
//! (Devices.Register).

mod support;

use std::time::Duration;

use prost::Message as _;
use protocol::device_identity::{INVITE_TTL, sign_device_certificate};
use support::clients::{TestUser, sign_certificate};
use support::harness::{TestPi, authed};
use support::{Check, bootstrap_admin, create_invite, pb, register};
use tonic::Code;

async fn counts(pi: &TestPi) -> (i64, i64) {
    pi.query(|db| {
        let users = db.query_row("SELECT count(*) FROM users", [], |r| r.get(0))?;
        let devices = db.query_row("SELECT count(*) FROM devices", [], |r| r.get(0))?;
        Ok((users, devices))
    })
    .await
}

// spec: device-identity/signup-with-an-invite
#[tokio::test]
async fn valid_invite_creates_user_and_device_and_is_used_up() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let code = create_invite(&conn, &admin, "alice")
        .await
        .invite
        .check("invite")
        .code;

    let alice = TestUser::new("alice");
    let mut phone = alice.new_device();
    let response = register(&conn, &code, &alice, &phone)
        .await
        .check("Register");
    assert_eq!(response.user_id, Some(alice.pb_user_id()));
    assert_eq!(counts(&pi).await, (2, 2));
    phone.authenticate(&conn).await;

    // device-identity: Signup with an invite / Used invite.
    let mallory = TestUser::new("alice");
    let again = register(&conn, &code, &mallory, &mallory.new_device()).await;
    assert_eq!(again.err().map(|s| s.code()), Some(Code::PermissionDenied));
    assert_eq!(counts(&pi).await, (2, 2), "a used invite creates nothing");
}

// spec: device-identity/signup-with-an-invite
#[tokio::test]
async fn unknown_revoked_or_expired_invite_creates_nothing() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let bob = TestUser::new("bob");
    let device = bob.new_device();

    let unknown = register(&conn, "no-such-code", &bob, &device).await;
    assert_eq!(
        unknown.err().map(|s| s.code()),
        Some(Code::PermissionDenied)
    );

    let revoked = create_invite(&conn, &admin, "bob")
        .await
        .invite
        .check("invite")
        .code;
    conn.admin()
        .revoke_invite(authed(
            admin.token(),
            pb::RevokeInviteRequest {
                code: revoked.clone(),
            },
        ))
        .await
        .check("RevokeInvite");
    let result = register(&conn, &revoked, &bob, &device).await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::PermissionDenied));

    let expiring = create_invite(&conn, &admin, "bob")
        .await
        .invite
        .check("invite")
        .code;
    pi.clock.advance(INVITE_TTL + Duration::from_secs(1));
    let result = register(&conn, &expiring, &bob, &device).await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::PermissionDenied));

    assert_eq!(counts(&pi).await, (1, 1));
}

// spec: device-identity/admin-owned-usernames
#[tokio::test]
async fn signup_uses_the_admins_username() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let invite = create_invite(&conn, &admin, "alice").await;
    assert_eq!(
        invite.payload.as_ref().and_then(|p| p.user_id.clone()),
        Some(TestUser::new("alice").pb_user_id())
    );
    let alice = TestUser::new("alice");
    let response = register(
        &conn,
        &invite.invite.check("invite").code,
        &alice,
        &alice.new_device(),
    )
    .await
    .check("Register");
    assert_eq!(
        response.user_id.check("user_id").username,
        "alice",
        "the user id is the one the invite names"
    );
    // No RPC changes a username: the stored name is the invite's.
    let names: Vec<String> = pi
        .query(|db| {
            let mut stmt = db.prepare("SELECT username FROM users ORDER BY username")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect()
        })
        .await;
    assert_eq!(names, vec!["akhil".to_owned(), "alice".to_owned()]);
}

// spec: device-identity/admin-owned-usernames
#[tokio::test]
async fn certificate_for_another_user_is_permission_denied_and_creates_nothing() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let code = create_invite(&conn, &admin, "alice")
        .await
        .invite
        .check("invite")
        .code;

    // A validly signed certificate, but naming mallory@pi.
    let mallory = TestUser::new("mallory");
    let device = mallory.new_device();
    let result = register(&conn, &code, &mallory, &device).await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::PermissionDenied));
    assert_eq!(counts(&pi).await, (1, 1));

    // The invite is still usable by the right certificate.
    let alice = TestUser::new("alice");
    register(&conn, &code, &alice, &alice.new_device())
        .await
        .check("Register with the right user");
}

// spec: device-identity/user-identity-key-and-device-certificates
#[tokio::test]
async fn certificate_signed_by_the_supplied_identity_key_is_accepted() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (admin, mut device) = bootstrap_admin(&pi, &conn).await;
    let stored: Vec<u8> = pi
        .query(|db| db.query_row("SELECT identity_public_key FROM users", [], |r| r.get(0)))
        .await;
    assert_eq!(stored, admin.identity_public_key());
    device.authenticate(&conn).await;
}

// spec: device-identity/user-identity-key-and-device-certificates
#[tokio::test]
async fn forged_certificate_is_permission_denied() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let admin = TestUser::new("akhil");
    // Signed by some other key than the identity key sent with it.
    let forger = TestUser::new("akhil");
    let device = forger.new_device();
    let result = register(&conn, &pi.bootstrap_code, &admin, &device).await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::PermissionDenied));
    assert_eq!(counts(&pi).await, (0, 0));
}

// spec: device-identity/federation-ready-identifiers
#[tokio::test]
async fn certificate_with_a_wrong_device_id_is_invalid() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let admin = TestUser::new("akhil");
    let mut device = admin.new_device();
    // Signed correctly, but the device id is not derived from the auth key.
    let other_auth = support::clients::new_key().verifying_key().to_bytes();
    let mut cert = sign_certificate(
        &admin.identity,
        admin.pb_user_id(),
        &other_auth,
        device.signer.public(),
    );
    let mut decoded =
        <pb::DeviceCertificate as prost::Message>::decode(cert.certificate.as_slice())
            .check("decode");
    if let Some(d) = decoded.device.as_mut() {
        d.device_id = device.device_id.to_vec();
    }
    cert.certificate = prost::Message::encode_to_vec(&decoded);
    cert.signature =
        protocol::device_identity::sign_device_certificate(&admin.identity, &cert.certificate)
            .to_vec();
    device.certificate = cert;
    let result = register(&conn, &pi.bootstrap_code, &admin, &device).await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::InvalidArgument));
    assert_eq!(counts(&pi).await, (0, 0));
}

// spec: device-identity/user-identity-key-and-device-certificates
#[tokio::test]
async fn signed_certificate_round_trips_as_opaque_bytes() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let code = create_invite(&conn, &admin, "alice")
        .await
        .invite
        .check("invite")
        .code;
    let alice = TestUser::new("alice");
    let mut phone = alice.new_device();

    // A certificate encoded with a field this version does not know (111,
    // varint 1). Decoding and re-encoding would drop it and break the
    // signature, so the Pi must keep the signed bytes exactly as sent.
    let mut bytes = phone.certificate.certificate.clone();
    bytes.extend_from_slice(&[0xf8, 0x06, 0x01]);
    let decoded = pb::DeviceCertificate::decode(&*bytes).check("decode certificate");
    assert_ne!(decoded.encode_to_vec(), bytes);
    phone.certificate = pb::SignedDeviceCertificate {
        signature: sign_device_certificate(&alice.identity, &bytes).to_vec(),
        certificate: bytes,
    };
    let wire = pb::SignedDeviceCertificate::decode(&*phone.certificate.encode_to_vec())
        .check("decode signed certificate");
    assert_eq!(wire, phone.certificate);

    register(&conn, &code, &alice, &phone)
        .await
        .check("Register with the extended certificate");
    phone.authenticate(&conn).await;
    let me = conn
        .devices()
        .get_self(authed(phone.token(), pb::GetSelfRequest {}))
        .await
        .check("GetSelf")
        .into_inner();
    assert_eq!(me.certificate.as_ref(), Some(&phone.certificate));

    // The same bytes reach other members with the KeyPackage.
    phone.upload_key_packages(&conn, 1).await;
    let fetched = conn
        .keys()
        .fetch_key_packages(authed(
            admin.token(),
            pb::FetchKeyPackagesRequest {
                user_id: Some(alice.pb_user_id()),
                device_ids: vec![],
                group_id: Vec::new(),
            },
        ))
        .await
        .check("FetchKeyPackages")
        .into_inner();
    let certificates: Vec<_> = fetched
        .devices
        .iter()
        .map(|d| d.certificate.as_ref())
        .collect();
    assert_eq!(certificates, vec![Some(&phone.certificate)]);
}

// spec: device-identity/bootstrap-admin
#[tokio::test]
async fn user_from_the_bootstrap_invite_is_the_admin() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let me = conn
        .devices()
        .get_self(authed(admin.token(), pb::GetSelfRequest {}))
        .await
        .check("GetSelf")
        .into_inner();
    assert!(me.is_admin);
    assert_eq!(me.user_id.check("user_id").username, "akhil");
    // The bootstrap invite is used up.
    let again = TestUser::new("akhil");
    let result = register(&conn, &pi.bootstrap_code, &again, &again.new_device()).await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::PermissionDenied));
}
