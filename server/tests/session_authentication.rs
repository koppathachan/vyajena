//! wire-transport: Session authentication (Devices.GetChallenge,
//! Devices.Authenticate, session tokens and the auth layer).

mod support;

use std::time::Duration;

use protocol::wire_transport::{CHALLENGE_NONCE_LEN, CHALLENGE_TTL, SESSION_TOKEN_MAX_LIFETIME};
use support::harness::{TestPi, authed};
use support::{Check, bootstrap_admin, pb};
use tonic::Code;

fn timestamp_secs(ts: Option<prost_types::Timestamp>) -> i64 {
    ts.check("timestamp").seconds
}

// spec: wire-transport/session-authentication
#[tokio::test]
async fn successful_authentication_returns_a_token_and_expiry() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;

    let challenge = conn
        .devices()
        .get_challenge(pb::GetChallengeRequest {})
        .await
        .check("GetChallenge")
        .into_inner();
    assert_eq!(challenge.nonce.len(), CHALLENGE_NONCE_LEN);
    let now = support::test_epoch()
        .duration_since(std::time::UNIX_EPOCH)
        .check("epoch")
        .as_secs() as i64;
    assert_eq!(
        timestamp_secs(challenge.expires_at),
        now + CHALLENGE_TTL.as_secs() as i64
    );

    let response = admin
        .answer(&conn, &challenge.nonce, &conn.exporter)
        .await
        .check("Authenticate");
    assert!(!response.session_token.is_empty());
    let expires = timestamp_secs(response.expires_at);
    assert!(expires > now && expires <= now + SESSION_TOKEN_MAX_LIFETIME.as_secs() as i64);
    // Token lifetime: six hours after issue, never later.
    assert_eq!(expires, now + 6 * 60 * 60);

    // The token works on an authenticated RPC.
    conn.devices()
        .get_self(authed(&response.session_token, pb::GetSelfRequest {}))
        .await
        .check("GetSelf with the new token");
}

// spec: wire-transport/session-authentication
#[tokio::test]
async fn tokens_are_stored_hashed() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let token = admin.token().to_owned();
    let stored: Vec<Vec<u8>> = pi
        .query(|db| {
            let mut stmt = db.prepare("SELECT token_sha256 FROM sessions")?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            rows.collect()
        })
        .await;
    assert_eq!(stored.len(), 1);
    assert_ne!(stored[0], token.as_bytes());
    assert_eq!(
        stored[0],
        server::auth::token_hash(&token).to_vec(),
        "the Pi keeps only SHA-256 of the token"
    );
}

// spec: wire-transport/session-authentication
#[tokio::test]
async fn replayed_challenge_is_unauthenticated() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let nonce = conn
        .devices()
        .get_challenge(pb::GetChallengeRequest {})
        .await
        .check("GetChallenge")
        .into_inner()
        .nonce;
    admin
        .answer(&conn, &nonce, &conn.exporter)
        .await
        .check("first answer");
    let replay = admin.answer(&conn, &nonce, &conn.exporter).await;
    assert_eq!(replay.err().map(|s| s.code()), Some(Code::Unauthenticated));
}

// spec: wire-transport/session-authentication
#[tokio::test]
async fn nonce_is_spent_by_a_failed_answer_too() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let nonce = conn
        .devices()
        .get_challenge(pb::GetChallengeRequest {})
        .await
        .check("GetChallenge")
        .into_inner()
        .nonce;
    let wrong = admin.answer(&conn, &nonce, &[0u8; 32]).await;
    assert_eq!(wrong.err().map(|s| s.code()), Some(Code::Unauthenticated));
    let retry = admin.answer(&conn, &nonce, &conn.exporter).await;
    assert_eq!(retry.err().map(|s| s.code()), Some(Code::Unauthenticated));
}

// spec: wire-transport/session-authentication
#[tokio::test]
async fn unknown_nonce_is_unauthenticated() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let made_up = admin.answer(&conn, &[5u8; 32], &conn.exporter).await;
    assert_eq!(made_up.err().map(|s| s.code()), Some(Code::Unauthenticated));
}

// spec: wire-transport/session-authentication
#[tokio::test]
async fn slow_answer_after_61_seconds_is_unauthenticated() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let nonce = conn
        .devices()
        .get_challenge(pb::GetChallengeRequest {})
        .await
        .check("GetChallenge")
        .into_inner()
        .nonce;
    pi.clock.advance(Duration::from_secs(61));
    let late = admin.answer(&conn, &nonce, &conn.exporter).await;
    assert_eq!(late.err().map(|s| s.code()), Some(Code::Unauthenticated));
}

// spec: wire-transport/session-authentication
#[tokio::test]
async fn answer_within_60_seconds_is_accepted() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let nonce = conn
        .devices()
        .get_challenge(pb::GetChallengeRequest {})
        .await
        .check("GetChallenge")
        .into_inner()
        .nonce;
    pi.clock.advance(Duration::from_secs(60));
    admin
        .answer(&conn, &nonce, &conn.exporter)
        .await
        .check("answer at 60 s");
}

// spec: wire-transport/session-authentication
#[tokio::test]
async fn signature_from_another_connection_is_unauthenticated() {
    let pi = TestPi::start().await;
    let first = pi.connect().await;
    let second = pi.connect().await;
    assert_ne!(first.exporter, second.exporter);
    let (_, admin) = bootstrap_admin(&pi, &first).await;

    // The challenge is issued and answered on `first`, but signed over the
    // exporter value of `second`.
    let replayed = admin.try_authenticate_with(&first, &second.exporter).await;
    assert_eq!(
        replayed.err().map(|s| s.code()),
        Some(Code::Unauthenticated)
    );
}

// spec: wire-transport/session-authentication
#[tokio::test]
async fn unregistered_device_is_unauthenticated() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let stranger = support::clients::TestUser::new("mallory").new_device();
    let result = stranger.try_authenticate_with(&conn, &conn.exporter).await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::Unauthenticated));
}

// spec: wire-transport/session-authentication
#[tokio::test]
async fn revoked_device_with_a_valid_signature_is_permission_denied() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let (alice, tablet) = support::invite_user(&conn, &admin, "alice").await;
    let phone = support::link_device(&conn, &alice, &tablet).await;
    revoke(&conn, &tablet, &phone).await;

    let result = phone.try_authenticate_with(&conn, &conn.exporter).await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::PermissionDenied));
}

// spec: wire-transport/session-authentication
#[tokio::test]
async fn bad_signature_for_a_revoked_device_is_unauthenticated() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let (alice, tablet) = support::invite_user(&conn, &admin, "alice").await;
    let phone = support::link_device(&conn, &alice, &tablet).await;
    revoke(&conn, &tablet, &phone).await;

    // Signed over a wrong exporter value: the signature does not check out,
    // so the answer must not reveal that the phone is revoked.
    let wrong = phone.try_authenticate_with(&conn, &[0u8; 32]).await;
    assert_eq!(wrong.err().map(|s| s.code()), Some(Code::Unauthenticated));
    // Someone else's key signing for the revoked phone's device id.
    let mut impostor = support::clients::TestUser::new("alice").new_device();
    impostor.device_id = phone.device_id;
    let forged = impostor.try_authenticate_with(&conn, &conn.exporter).await;
    assert_eq!(forged.err().map(|s| s.code()), Some(Code::Unauthenticated));
}

async fn revoke(
    conn: &support::harness::Conn,
    by: &support::clients::TestDevice,
    target: &support::clients::TestDevice,
) {
    conn.devices()
        .revoke_device(authed(
            by.token(),
            pb::RevokeDeviceRequest {
                device_id: target.device_id.to_vec(),
            },
        ))
        .await
        .check("RevokeDevice");
}

// spec: wire-transport/session-authentication
#[tokio::test]
async fn missing_or_malformed_token_is_unauthenticated() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, _admin) = bootstrap_admin(&pi, &conn).await;

    let missing = conn.devices().get_self(pb::GetSelfRequest {}).await;
    assert_eq!(missing.err().map(|s| s.code()), Some(Code::Unauthenticated));

    let made_up = conn
        .devices()
        .get_self(authed("not-a-token", pb::GetSelfRequest {}))
        .await;
    assert_eq!(made_up.err().map(|s| s.code()), Some(Code::Unauthenticated));

    let mut no_bearer = tonic::Request::new(pb::GetSelfRequest {});
    no_bearer
        .metadata_mut()
        .insert("authorization", "Basic abc".parse().check("metadata"));
    let no_bearer = conn.devices().get_self(no_bearer).await;
    assert_eq!(
        no_bearer.err().map(|s| s.code()),
        Some(Code::Unauthenticated)
    );
}

// spec: wire-transport/session-authentication
#[tokio::test]
async fn expired_token_is_unauthenticated() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    pi.clock
        .advance(SESSION_TOKEN_MAX_LIFETIME - Duration::from_secs(1));
    conn.devices()
        .get_self(authed(admin.token(), pb::GetSelfRequest {}))
        .await
        .check("GetSelf just before expiry");
    pi.clock.advance(Duration::from_secs(1));
    let expired = conn
        .devices()
        .get_self(authed(admin.token(), pb::GetSelfRequest {}))
        .await;
    assert_eq!(expired.err().map(|s| s.code()), Some(Code::Unauthenticated));
}

// spec: wire-transport/session-authentication
#[tokio::test]
async fn every_other_rpc_needs_a_token() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, _admin) = bootstrap_admin(&pi, &conn).await;
    let unauthenticated = |result: Result<(), tonic::Status>| {
        assert_eq!(result.err().map(|s| s.code()), Some(Code::Unauthenticated));
    };
    let devices = conn.devices();
    unauthenticated(
        devices
            .clone()
            .link_device(pb::LinkDeviceRequest::default())
            .await
            .map(drop),
    );
    unauthenticated(
        devices
            .clone()
            .revoke_device(pb::RevokeDeviceRequest::default())
            .await
            .map(drop),
    );
    unauthenticated(
        devices
            .clone()
            .update_push_token(pb::UpdatePushTokenRequest::default())
            .await
            .map(drop),
    );
    let keys = conn.keys();
    unauthenticated(
        keys.clone()
            .upload_key_packages(pb::UploadKeyPackagesRequest::default())
            .await
            .map(drop),
    );
    unauthenticated(
        keys.clone()
            .fetch_key_packages(pb::FetchKeyPackagesRequest::default())
            .await
            .map(drop),
    );
    let admin = conn.admin();
    unauthenticated(
        admin
            .clone()
            .create_invite(pb::CreateInviteRequest::default())
            .await
            .map(drop),
    );
    unauthenticated(
        admin
            .clone()
            .list_invites(pb::ListInvitesRequest::default())
            .await
            .map(drop),
    );
    unauthenticated(
        admin
            .clone()
            .revoke_invite(pb::RevokeInviteRequest::default())
            .await
            .map(drop),
    );
    unauthenticated(
        admin
            .clone()
            .create_group(pb::CreateGroupRequest::default())
            .await
            .map(drop),
    );
    unauthenticated(
        admin
            .clone()
            .remove_user(pb::RemoveUserRequest::default())
            .await
            .map(drop),
    );
}
