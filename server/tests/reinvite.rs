//! device-identity: Re-inviting an existing user. An admin-minted invite for a
//! present, non-removed username authorizes a server-side identity re-key on
//! join, with no prior `Admin.RemoveUser` (a reinstalled-device recovery).

mod support;

use rusqlite::params;
use support::clients::TestUser;
use support::harness::TestServer;
use support::{Check, bootstrap_admin, create_invite, invite_user, register};
use tonic::Code;

/// The identity public key pinned for `username`.
async fn identity_key(server: &TestServer, username: &'static str) -> Vec<u8> {
    server
        .query(move |conn| {
            conn.query_row(
                "SELECT identity_public_key FROM users WHERE username = ?1",
                params![username],
                |row| row.get(0),
            )
        })
        .await
}

/// Total devices for `username` and how many of them are revoked.
async fn device_counts(server: &TestServer, username: &'static str) -> (i64, i64) {
    server
        .query(move |conn| {
            let total = conn.query_row(
                "SELECT count(*) FROM devices WHERE username = ?1",
                params![username],
                |r| r.get(0),
            )?;
            let revoked = conn.query_row(
                "SELECT count(*) FROM devices WHERE username = ?1 AND revoked_at IS NOT NULL",
                params![username],
                |r| r.get(0),
            )?;
            Ok((total, revoked))
        })
        .await
}

async fn user_count(server: &TestServer) -> i64 {
    server
        .query(|conn| conn.query_row("SELECT count(*) FROM users", [], |r| r.get(0)))
        .await
}

// spec: device-identity/re-inviting-an-existing-user
#[tokio::test]
async fn admin_can_re_invite_a_present_user() {
    let server = TestServer::start().await;
    let conn = server.connect().await;
    let (_admin, admin) = bootstrap_admin(&server, &conn).await;
    // alice is present and non-removed.
    let (_alice, _phone) = invite_user(&conn, &admin, "alice").await;

    // Minting an invite for the present username succeeds (a re-invite), rather
    // than failing with ALREADY_EXISTS. create_invite panics on an RPC error.
    let response = create_invite(&conn, &admin, "alice").await;
    let invite = response.invite.check("re-invite");
    assert_eq!(
        invite.user_id.check("user_id").username,
        "alice",
        "the re-invite is bound to the same username"
    );
}

// spec: device-identity/re-inviting-an-existing-user
#[tokio::test]
async fn re_invite_re_keys_the_user_and_revokes_prior_devices() {
    let server = TestServer::start().await;
    let conn = server.connect().await;
    let (_admin, admin) = bootstrap_admin(&server, &conn).await;
    let (alice, old_phone) = invite_user(&conn, &admin, "alice").await;
    let old_key = alice.identity_public_key();
    assert_eq!(identity_key(&server, "alice").await, old_key);

    // Re-invite, then a reinstalled device with a FRESH identity key registers.
    let code = create_invite(&conn, &admin, "alice")
        .await
        .invite
        .check("re-invite")
        .code;
    let alice2 = TestUser::new("alice");
    let new_key = alice2.identity_public_key();
    assert_ne!(new_key, old_key, "the reinstalled device re-keys");
    let mut new_phone = alice2.new_device();
    let response = register(&conn, &code, &alice2, &new_phone)
        .await
        .check("Register (re-invite re-key)");

    // Same user id, no new user row.
    assert_eq!(response.user_id, Some(alice2.pb_user_id()));
    assert_eq!(user_count(&server).await, 2, "akhil + alice, no new user");
    // The server installed the new identity key for the same username.
    assert_eq!(identity_key(&server, "alice").await, new_key);

    // Every prior device is revoked; the new device is active.
    let (total, revoked) = device_counts(&server, "alice").await;
    assert_eq!(total, 2, "old phone row kept plus the new device");
    assert_eq!(revoked, 1, "the one prior device is revoked");

    // The surviving old device is kicked: it can no longer authenticate.
    let old = old_phone.try_authenticate_with(&conn, &conn.exporter).await;
    assert_eq!(
        old.err().map(|s| s.code()),
        Some(Code::PermissionDenied),
        "the re-keyed-away device cannot authenticate"
    );
    // The reinstalled device authenticates.
    new_phone.authenticate(&conn).await;
}

// spec: device-identity/re-inviting-an-existing-user
#[tokio::test]
async fn a_re_invite_is_single_use() {
    let server = TestServer::start().await;
    let conn = server.connect().await;
    let (_admin, admin) = bootstrap_admin(&server, &conn).await;
    let (_alice, _old_phone) = invite_user(&conn, &admin, "alice").await;

    let code = create_invite(&conn, &admin, "alice")
        .await
        .invite
        .check("re-invite")
        .code;
    let alice2 = TestUser::new("alice");
    register(&conn, &code, &alice2, &alice2.new_device())
        .await
        .check("first re-key");
    let key_after_first = identity_key(&server, "alice").await;

    // A second register with the same (now used) invite is rejected and re-keys
    // nothing.
    let alice3 = TestUser::new("alice");
    let again = register(&conn, &code, &alice3, &alice3.new_device()).await;
    assert_eq!(again.err().map(|s| s.code()), Some(Code::PermissionDenied));
    assert_eq!(
        identity_key(&server, "alice").await,
        key_after_first,
        "a used invite changes no identity key"
    );
}

// spec: device-identity/re-inviting-an-existing-user
#[tokio::test]
async fn existing_user_without_a_valid_invite_cannot_re_key() {
    let server = TestServer::start().await;
    let conn = server.connect().await;
    let (_admin, admin) = bootstrap_admin(&server, &conn).await;
    let (alice, _old_phone) = invite_user(&conn, &admin, "alice").await;
    let key_before = alice.identity_public_key();

    // An unknown invite code for the present user: rejected before the user row
    // is examined, nothing re-keyed.
    let intruder = TestUser::new("alice");
    let result = register(&conn, "no-such-code", &intruder, &intruder.new_device()).await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::PermissionDenied));
    assert_eq!(
        identity_key(&server, "alice").await,
        key_before,
        "no invite, no re-key"
    );
    let (_total, revoked) = device_counts(&server, "alice").await;
    assert_eq!(revoked, 0, "alice's device is untouched");
}
