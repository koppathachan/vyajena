//! account-deletion: a user deletes their own account (Devices.DeleteAccount).
//! The server revokes the caller's devices, frees the username, evicts them from
//! every group, closes their DMs, deletes their server-held data; the admin is
//! refused and can never be reset over the network.

mod support;

use rusqlite::{OptionalExtension, params};
use support::clients::TestDevice;
use support::harness::{TestServer, authed};
use support::messaging::{GROUP_ID, World, commit_envelope_in, world};
use support::pb;
use support::pb::envelope::Kind;
use support::{
    Check, bootstrap_admin, create_invite, invite_user, link_device, register,
    server_external_sender,
};
use tonic::Code;

/// `Devices.DeleteAccount` as `device`.
async fn delete_account(
    conn: &support::harness::Conn,
    device: &TestDevice,
) -> Result<(), tonic::Status> {
    conn.devices()
        .delete_account(authed(device.token(), pb::DeleteAccountRequest {}))
        .await
        .map(|_| ())
}

/// Whether `username` still has a user row (`Some`) and, if so, whether it is
/// marked removed.
async fn user_marker(server: &TestServer, username: &'static str) -> Option<bool> {
    server
        .query(move |conn| {
            conn.query_row(
                "SELECT removed_at IS NOT NULL FROM users WHERE username = ?1",
                params![username],
                |row| row.get::<_, bool>(0),
            )
            .optional()
        })
        .await
}

// spec: account-deletion/user-deletes-their-own-account
#[tokio::test]
async fn member_deletes_their_own_account() {
    let server = TestServer::start().await;
    let conn = server.connect().await;
    let (_admin, admin) = bootstrap_admin(&server, &conn).await;
    let (_alice, laptop) = invite_user(&conn, &admin, "alice").await;
    let (_bob, phone) = invite_user(&conn, &admin, "bob").await;

    delete_account(&conn, &laptop).await.check("DeleteAccount");

    // Alice is marked removed and her device is revoked: it cannot authenticate.
    assert_eq!(user_marker(&server, "alice").await, Some(true));
    let again = laptop.try_authenticate_with(&conn, &conn.exporter).await;
    assert_eq!(
        again.err().map(|s| s.code()),
        Some(Code::PermissionDenied),
        "the deleted device cannot authenticate"
    );

    // Only Alice is gone: the request carries no target id, so Bob is untouched.
    assert_eq!(user_marker(&server, "bob").await, Some(false));
    conn.devices()
        .get_self(authed(phone.token(), pb::GetSelfRequest {}))
        .await
        .check("bob still works");
}

// spec: account-deletion/user-deletes-their-own-account
#[tokio::test]
async fn unauthenticated_delete_is_rejected() {
    let server = TestServer::start().await;
    let conn = server.connect().await;
    let (_admin, _admin_device) = bootstrap_admin(&server, &conn).await;

    // No bearer token: the call is rejected and nothing is deleted.
    let refused = conn
        .devices()
        .delete_account(tonic::Request::new(pb::DeleteAccountRequest {}))
        .await;
    assert_eq!(refused.err().map(|s| s.code()), Some(Code::Unauthenticated));
}

// spec: account-deletion/the-admin-cannot-self-delete-through-the-rpc
#[tokio::test]
async fn the_admin_cannot_self_delete() {
    let server = TestServer::start().await;
    let conn = server.connect().await;
    let (_admin, admin) = bootstrap_admin(&server, &conn).await;

    let refused = delete_account(&conn, &admin).await;
    assert_eq!(
        refused.err().map(|s| s.code()),
        Some(Code::FailedPrecondition),
        "the admin's self-delete is refused"
    );

    // The admin account, its device and nothing else changed: it still works.
    assert_eq!(user_marker(&server, "akhil").await, Some(false));
    conn.devices()
        .get_self(authed(admin.token(), pb::GetSelfRequest {}))
        .await
        .check("the admin is unchanged");
}

// spec: account-deletion/what-the-server-deletes-and-what-it-retains
#[tokio::test]
async fn server_held_data_is_deleted_and_a_minimal_marker_is_kept() {
    let server = TestServer::start().await;
    let conn = server.connect().await;
    let (_admin, admin) = bootstrap_admin(&server, &conn).await;
    let (_alice, laptop) = invite_user(&conn, &admin, "alice").await;
    laptop.upload_key_packages(&conn, 5).await;
    let laptop_id = laptop.device_id;
    // A stored push token to prove it is cleared (set directly to skip the
    // platform token-format validation, which is not what this test is about).
    server
        .query(move |conn| {
            conn.execute(
                "UPDATE devices SET push_platform = 1, push_token = 'tok' WHERE device_id = ?1",
                params![laptop_id.as_slice()],
            )
        })
        .await;

    delete_account(&conn, &laptop).await.check("DeleteAccount");

    let (key_packages, push): (i64, Option<String>) = server
        .query(move |conn| {
            let key_packages = conn.query_row(
                "SELECT count(*) FROM key_packages WHERE device_id = ?1",
                params![laptop_id.as_slice()],
                |r| r.get(0),
            )?;
            let push = conn.query_row(
                "SELECT push_token FROM devices WHERE device_id = ?1",
                params![laptop_id.as_slice()],
                |r| r.get::<_, Option<String>>(0),
            )?;
            Ok((key_packages, push))
        })
        .await;
    assert_eq!(key_packages, 0, "the user's key packages are deleted");
    assert_eq!(push, None, "the user's push token is cleared");

    // Only a minimal removed marker is kept, so the username can be re-issued: a
    // new invite for it succeeds and a fresh device registers against it.
    assert_eq!(user_marker(&server, "alice").await, Some(true));
    let code = create_invite(&conn, &admin, "alice")
        .await
        .invite
        .check("re-issue")
        .code;
    let alice2 = support::clients::TestUser::new("alice");
    let device2 = alice2.new_device();
    register(&conn, &code, &alice2, &device2)
        .await
        .check("register against the freed username");
}

// spec: account-deletion/the-initiating-device-wipes-itself-on-success
#[tokio::test]
async fn another_device_of_the_user_learns_it_was_removed() {
    let server = TestServer::start().await;
    let conn = server.connect().await;
    let (_admin, admin) = bootstrap_admin(&server, &conn).await;
    let (alice, laptop) = invite_user(&conn, &admin, "alice").await;
    let tablet = link_device(&conn, &alice, &laptop).await;

    delete_account(&conn, &laptop).await.check("DeleteAccount");

    // The other device, now revoked too, learns it was removed through the
    // authoritative signal: its next Authenticate is PERMISSION_DENIED.
    let auth = tablet.try_authenticate_with(&conn, &conn.exporter).await;
    assert_eq!(auth.err().map(|s| s.code()), Some(Code::PermissionDenied));
}

const DM_ID: &[u8] = b"alice-and-bob-dm";

fn dm_users() -> (&'static str, &'static str) {
    ("alice@chat", "bob@chat")
}

/// Alice's laptop opens the Alice–Bob DM and Bob's phone joins it (mirrors the
/// `dms` tests' setup).
async fn open_and_join_dm(w: &mut World) {
    w.laptop.upload_dm_key_packages(&w.conn, 10).await;
    w.phone.upload_dm_key_packages(&w.conn, 10).await;
    let server = server_external_sender(&w.conn).await;
    let request = w
        .laptop
        .create_dm(DM_ID, &server, dm_users(), w.bob.pb_user_id());
    w.conn
        .dms()
        .create_dm(authed(w.laptop.token(), request))
        .await
        .check("CreateDm");
    let key_packages: Vec<Vec<u8>> = w
        .conn
        .keys()
        .fetch_key_packages(authed(
            w.laptop.token(),
            pb::FetchKeyPackagesRequest {
                user_id: Some(w.bob.pb_user_id()),
                device_ids: vec![],
                group_id: DM_ID.to_vec(),
            },
        ))
        .await
        .check("fetch for the DM")
        .into_inner()
        .devices
        .into_iter()
        .map(|d| d.key_package)
        .collect();
    let epoch = w.laptop.epoch();
    let (commit, welcome) = w.laptop.add(&key_packages);
    w.laptop_stream
        .accepted(commit_envelope_in(
            DM_ID,
            epoch,
            commit,
            Some((welcome, &[&w.phone])),
        ))
        .await;
    w.laptop.confirm();
    let got = w.phone_stream.envelope().await;
    assert_eq!(got.kind, Kind::Welcome as i32);
    w.phone.swap_groups();
    w.phone.join(&got.ciphertext);
    w.phone_stream.ack_in(DM_ID, got.server_seq).await;
}

// spec: account-deletion/deletion-evicts-the-user-from-every-group-and-closes-their-dms
#[tokio::test]
async fn deletion_evicts_from_the_group_and_closes_the_dm() {
    let mut w = world().await;
    open_and_join_dm(&mut w).await;
    let laptop = w.laptop.device_id;
    let phone = w.phone.device_id;

    delete_account(&w.conn, &w.laptop)
        .await
        .check("DeleteAccount");

    let (removes, inactive, deliveries, dm_notices) = w
        .server
        .query(move |conn| {
            let removes: i64 = conn.query_row(
                "SELECT count(*) FROM pi_removes WHERE device_id = ?1 AND group_id = ?2",
                params![laptop.as_slice(), GROUP_ID],
                |r| r.get(0),
            )?;
            let inactive: i64 = conn.query_row(
                "SELECT count(*) FROM inactive_devices WHERE device_id = ?1",
                params![laptop.as_slice()],
                |r| r.get(0),
            )?;
            let deliveries: i64 = conn.query_row(
                "SELECT count(*) FROM deliveries WHERE device_id = ?1",
                params![laptop.as_slice()],
                |r| r.get(0),
            )?;
            let dm_notices: i64 = conn.query_row(
                "SELECT count(*) FROM dm_closed_notices WHERE device_id = ?1",
                params![phone.as_slice()],
                |r| r.get(0),
            )?;
            Ok((removes, inactive, deliveries, dm_notices))
        })
        .await;

    assert!(
        removes >= 1,
        "an external Remove was issued for alice in the main group"
    );
    assert_eq!(inactive, 1, "alice's device is marked inactive");
    assert_eq!(deliveries, 0, "alice's queued entries are deleted");
    assert_eq!(dm_notices, 1, "bob is owed a DmClosed for the closed DM");
}
