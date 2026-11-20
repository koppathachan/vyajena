//! key-directory: Identity key lookup (Keys.GetIdentityKeys, client-core
//! task 5.5).

mod support;

use support::clients::TestDevice;
use support::harness::{Conn, TestServer, authed};
use support::{Check, bootstrap_admin, invite_user, link_device, pb};
use tonic::Code;

async fn lookup(
    conn: &Conn,
    caller: &TestDevice,
    users: Vec<pb::UserId>,
) -> Result<pb::GetIdentityKeysResponse, tonic::Status> {
    Ok(conn
        .keys()
        .get_identity_keys(authed(
            caller.token(),
            pb::GetIdentityKeysRequest { user_ids: users },
        ))
        .await?
        .into_inner())
}

async fn one_time_count(server: &TestServer, device: &TestDevice) -> u32 {
    let id = device.device_id;
    server
        .state
        .store
        .call(move |db| server::store::records::one_time_key_packages(db, &id))
        .await
        .check("count")
}

fn user_id(username: &str, server_id: &str) -> pb::UserId {
    pb::UserId {
        username: username.to_owned(),
        server_id: server_id.to_owned(),
    }
}

// spec: key-directory/identity-key-lookup
#[tokio::test]
async fn learning_keys_without_a_keypackage() {
    let server = TestServer::start().await;
    let conn = server.connect().await;
    let (_, admin) = bootstrap_admin(&server, &conn).await;
    let (alice_user, alice) = invite_user(&conn, &admin, "alice").await;
    let (bob_user, bob) = invite_user(&conn, &admin, "bob").await;
    let (_, carol) = invite_user(&conn, &admin, "carol").await;
    alice.upload_key_packages(&conn, 3).await;
    bob.upload_key_packages(&conn, 3).await;

    // Carol joined from a Welcome and looks up Alice's and Bob's keys (Bob
    // listed twice: one entry per distinct user, in request order).
    let response = lookup(
        &conn,
        &carol,
        vec![
            alice_user.pb_user_id(),
            bob_user.pb_user_id(),
            bob_user.pb_user_id(),
        ],
    )
    .await
    .check("GetIdentityKeys");
    let keys: Vec<(pb::UserId, Vec<u8>)> = response
        .keys
        .into_iter()
        .map(|k| (k.user_id.check("user id"), k.identity_public_key))
        .collect();
    assert_eq!(
        keys,
        vec![
            (alice_user.pb_user_id(), alice_user.identity_public_key()),
            (bob_user.pb_user_id(), bob_user.identity_public_key()),
        ]
    );
    assert_eq!(one_time_count(&server, &alice).await, 3, "nothing consumed");
    assert_eq!(one_time_count(&server, &bob).await, 3, "nothing consumed");

    // An empty list asks for nothing.
    let empty = lookup(&conn, &carol, vec![]).await.check("empty list");
    assert!(empty.keys.is_empty());
}

// spec: key-directory/identity-key-lookup
#[tokio::test]
async fn unknown_user_in_the_list_is_not_found() {
    let server = TestServer::start().await;
    let conn = server.connect().await;
    let (_, admin) = bootstrap_admin(&server, &conn).await;
    let (alice_user, _alice) = invite_user(&conn, &admin, "alice").await;

    let result = lookup(
        &conn,
        &admin,
        vec![alice_user.pb_user_id(), user_id("zed", "chat")],
    )
    .await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::NotFound));

    // A user of another server is just as unknown.
    let result = lookup(&conn, &admin, vec![user_id("alice", "other-server")]).await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::NotFound));
}

// spec: key-directory/identity-key-lookup
#[tokio::test]
async fn user_without_a_non_revoked_device_is_not_found() {
    let server = TestServer::start().await;
    let conn = server.connect().await;
    let (_, admin) = bootstrap_admin(&server, &conn).await;
    let (bob_user, tablet) = invite_user(&conn, &admin, "bob").await;
    let phone = link_device(&conn, &bob_user, &tablet).await;

    // Revoking one of two devices leaves the key visible.
    conn.devices()
        .revoke_device(authed(
            tablet.token(),
            pb::RevokeDeviceRequest {
                device_id: phone.device_id.to_vec(),
            },
        ))
        .await
        .check("RevokeDevice");
    let response = lookup(&conn, &admin, vec![bob_user.pb_user_id()])
        .await
        .check("one device left");
    assert_eq!(response.keys.len(), 1);

    // Removing Bob revokes his last device: his key is no longer handed out.
    conn.admin()
        .remove_user(authed(
            admin.token(),
            pb::RemoveUserRequest {
                user_id: Some(bob_user.pb_user_id()),
            },
        ))
        .await
        .check("RemoveUser");
    let result = lookup(&conn, &admin, vec![bob_user.pb_user_id()]).await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::NotFound));
}

// spec: key-directory/identity-key-lookup
#[tokio::test]
async fn lookup_needs_a_session() {
    let server = TestServer::start().await;
    let conn = server.connect().await;
    let (admin_user, _admin) = bootstrap_admin(&server, &conn).await;
    let result = conn
        .keys()
        .get_identity_keys(pb::GetIdentityKeysRequest {
            user_ids: vec![admin_user.pb_user_id()],
        })
        .await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::Unauthenticated));
}
