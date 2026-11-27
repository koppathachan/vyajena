//! message-delivery: Content-free wake for an own-access admin-event. An offline
//! device whose own user was removed from a named group, whose named group was
//! closed, or who was added to a new named group gets one content-free wake push
//! although the triggering entry is control traffic; a commit that only re-keys
//! the group wakes nothing (unchanged from `push-kind-aware`).
//!
//! Cast (support::messaging::World): the admin (`akhil`), Alice's laptop and
//! Bob's phone. Bob's phone has a push token and is offline when it happens.

mod support;

use std::time::Duration;

use support::Check;
use support::clients::TestDevice;
use support::harness::{Conn, TestServer, authed};
use support::messaging::{World, commit_envelope_in, offline, wait_offline, world};
use support::pb;
use support::pb::envelope::Kind;
use support::server_external_sender;

const NAMED: &[u8] = b"weekend-named-gr";

async fn set_token(conn: &Conn, device: &TestDevice, token: &str) {
    conn.devices()
        .update_push_token(authed(
            device.token(),
            pb::UpdatePushTokenRequest {
                push_token: Some(pb::PushToken {
                    platform: pb::PushPlatform::Fcm as i32,
                    token: token.to_owned(),
                }),
            },
        ))
        .await
        .check("UpdatePushToken");
}

/// Waits for `device` to have `expected` pings, then checks no more come.
async fn pings_settle(server: &TestServer, device: &TestDevice, expected: usize) {
    let deadline = tokio::time::Instant::now() + support::messaging::WAIT;
    while server.pushes.count(&device.device_id) < expected {
        assert!(
            tokio::time::Instant::now() < deadline,
            "{} of {expected} pings",
            server.pushes.count(&device.device_id)
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(server.pushes.count(&device.device_id), expected);
}

async fn v3_world() -> World {
    let w = world().await;
    w.phone.upload_named_key_packages(&w.conn, 10).await;
    w
}

async fn create_named(w: &mut World) {
    let server = server_external_sender(&w.conn).await;
    let request = w
        .admin
        .create_named_group(NAMED, &server, "akhil@chat", true);
    w.conn
        .admin()
        .create_named_group(authed(w.admin.token(), request))
        .await
        .check("CreateNamedGroup");
}

/// The admin's commit adding Bob's phone to the named group, with its Welcome.
async fn add_bob(w: &mut World) {
    let key_packages: Vec<Vec<u8>> = w
        .conn
        .keys()
        .fetch_key_packages(authed(
            w.admin.token(),
            pb::FetchKeyPackagesRequest {
                user_id: Some(w.bob.pb_user_id()),
                device_ids: vec![],
                group_id: NAMED.to_vec(),
            },
        ))
        .await
        .check("fetch for the named group")
        .into_inner()
        .devices
        .into_iter()
        .map(|d| d.key_package)
        .collect();
    let epoch = w.admin.epoch();
    let (commit, welcome) = w.admin.add(&key_packages);
    w.admin_stream
        .accepted(commit_envelope_in(
            NAMED,
            epoch,
            commit,
            Some((welcome, &[&w.phone])),
        ))
        .await;
    w.admin.confirm();
}

/// Bob's phone, in the named group, is offline with a push token.
async fn bob_in_named_and_offline() -> World {
    let mut w = v3_world().await;
    create_named(&mut w).await;
    add_bob(&mut w).await;
    let got = w.phone_stream.envelope().await;
    assert_eq!(got.kind, Kind::Welcome as i32);
    w.phone.swap_groups();
    w.phone.join(&got.ciphertext);
    w.phone_stream.ack_in(NAMED, got.server_seq).await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;
    w
}

// spec: message-delivery/content-free-wake-for-an-own-access-admin-event
#[tokio::test]
async fn removal_wakes_the_removed_device() {
    let mut w = bob_in_named_and_offline().await;
    let leaf = w.phone.leaf();
    let epoch = w.admin.epoch();
    let commit = w.admin.remove(&[leaf]);
    w.admin_stream
        .accepted(commit_envelope_in(NAMED, epoch, commit, None))
        .await;
    w.admin.confirm();
    pings_settle(&w.server, &w.phone, 1).await;
}

// spec: message-delivery/content-free-wake-for-an-own-access-admin-event
#[tokio::test]
async fn a_commit_that_only_re_keys_wakes_nobody() {
    let mut w = bob_in_named_and_offline().await;
    let epoch = w.admin.epoch();
    let commit = w.admin.self_update();
    w.admin_stream
        .accepted(commit_envelope_in(NAMED, epoch, commit, None))
        .await;
    w.admin.confirm();
    pings_settle(&w.server, &w.phone, 0).await;
}

// spec: message-delivery/content-free-wake-for-an-own-access-admin-event
#[tokio::test]
async fn closing_a_group_wakes_its_offline_members() {
    let w = bob_in_named_and_offline().await;
    w.conn
        .admin()
        .close_group(authed(
            w.admin.token(),
            pb::CloseGroupRequest {
                group_id: NAMED.to_vec(),
            },
        ))
        .await
        .check("CloseGroup");
    pings_settle(&w.server, &w.phone, 1).await;
}

// spec: message-delivery/content-free-wake-for-an-own-access-admin-event
#[tokio::test]
async fn being_added_to_a_new_named_group_wakes_the_device() {
    let mut w = v3_world().await;
    create_named(&mut w).await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;
    add_bob(&mut w).await;
    pings_settle(&w.server, &w.phone, 1).await;
}
