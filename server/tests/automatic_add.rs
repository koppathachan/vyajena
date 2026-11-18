//! group-membership: Automatic add of new users, the Pi's side: who gets a
//! `JoinNotice`, when, and until when (flutter-app task 4.4). The admin's
//! device adding on the notice is tested in the core (client/tests).

mod support;

use support::clients::{TestDevice, TestUser};
use support::harness::authed;
use support::messaging::{
    GROUP_ID, Stream, World, add_over_stream, commit_envelope, offline, receive, reconnect,
    wait_offline, world,
};
use support::pb::envelope::Kind;
use support::{Check, create_invite, link_device, pb, register};

/// Carol registers from the admin's invite, without KeyPackages yet.
async fn carol_registers(w: &World) -> (TestUser, TestDevice) {
    let invite = create_invite(&w.conn, &w.admin, "carol").await;
    let carol = TestUser::new("carol");
    let mut phone = carol.new_device();
    register(&w.conn, &invite.invite.check("invite").code, &carol, &phone)
        .await
        .check("Register");
    phone.authenticate(&w.conn).await;
    (carol, phone)
}

fn names(notice: &pb::JoinNotice, device: &TestDevice) {
    assert_eq!(notice.group_id, GROUP_ID);
    assert_eq!(notice.device.as_ref(), Some(&device.device_ref()));
}

/// The KeyPackage of `device` only, as the admin's device fetches it.
async fn fetch_device(
    w: &World,
    caller: &TestDevice,
    user: &TestUser,
    device: &TestDevice,
) -> Vec<Vec<u8>> {
    w.conn
        .keys()
        .fetch_key_packages(authed(
            caller.token(),
            pb::FetchKeyPackagesRequest {
                user_id: Some(user.pb_user_id()),
                device_ids: vec![device.device_id.to_vec()],
                group_id: Vec::new(),
            },
        ))
        .await
        .check("FetchKeyPackages")
        .into_inner()
        .devices
        .into_iter()
        .map(|d| d.key_package)
        .collect()
}

// spec: group-membership/automatic-add-of-new-users
#[tokio::test]
async fn bob_joins_while_the_admin_is_online() {
    let mut w = world().await;
    let (carol, mut phone) = carol_registers(&w).await;
    // Registered but no KeyPackages yet: nothing the admin could add.
    w.admin_stream.assert_no_join_notice().await;

    phone.upload_key_packages(&w.conn, 10).await;
    let notice = w.admin_stream.join_notice().await;
    names(&notice, &phone);
    // Only the admin's devices hear about it.
    w.laptop_stream.assert_no_join_notice().await;
    w.phone_stream.assert_no_join_notice().await;

    // The admin's device adds it: KeyPackage for that device only, Welcome
    // in the same COMMIT.
    let mut carol_stream = Stream::open(&w.conn, &phone).await;
    let key_packages = fetch_device(&w, &w.admin, &carol, &phone).await;
    assert_eq!(key_packages.len(), 1);
    add_over_stream(
        &mut w.admin,
        &mut w.admin_stream,
        &key_packages,
        &mut [
            (&mut w.laptop, &mut w.laptop_stream),
            (&mut w.phone, &mut w.phone_stream),
        ],
        &mut [(&mut phone, &mut carol_stream)],
    )
    .await;

    // A member is never announced again.
    w.admin_stream = reconnect(&w.conn, &mut w.admin).await;
    w.admin_stream.assert_no_join_notice().await;
}

// spec: group-membership/automatic-add-of-new-users
#[tokio::test]
async fn admin_offline_when_bob_joins_hears_on_connect() {
    let mut w = world().await;
    w.admin_stream = offline(&w.conn, &w.admin).await;
    wait_offline(&w.pi, &w.admin).await;
    let (_, phone) = carol_registers(&w).await;
    phone.upload_key_packages(&w.conn, 10).await;

    // The admin's phone connects: it hears about Carol's phone, and again on
    // every connect until it is a member.
    for _ in 0..2 {
        w.admin.authenticate(&w.conn).await;
        let mut stream = Stream::open(&w.conn, &w.admin).await;
        names(&stream.join_notice().await, &phone);
        w.admin_stream = stream;
    }
    // Members that are not the admin's never do.
    w.laptop_stream = reconnect(&w.conn, &mut w.laptop).await;
    w.laptop_stream.assert_no_join_notice().await;
}

// spec: group-membership/automatic-add-of-new-users
#[tokio::test]
async fn devices_that_are_not_new_are_not_announced() {
    let mut w = world().await;
    // A device linked to a user who already has a member device: the user's
    // own device adds it (device-identity: Device linking).
    let tablet = link_device(&w.conn, &w.bob, &w.phone).await;
    tablet.upload_key_packages(&w.conn, 10).await;
    // A new user the admin removed before anyone added her.
    let (carol, carol_phone) = carol_registers(&w).await;
    carol_phone.upload_key_packages(&w.conn, 10).await;
    names(&w.admin_stream.join_notice().await, &carol_phone);
    w.conn
        .admin()
        .remove_user(authed(
            w.admin.token(),
            pb::RemoveUserRequest {
                user_id: Some(carol.pb_user_id()),
            },
        ))
        .await
        .check("RemoveUser");

    w.admin_stream = reconnect(&w.conn, &mut w.admin).await;
    w.admin_stream.assert_no_join_notice().await;
}

// spec: group-membership/automatic-add-of-new-users
#[tokio::test]
async fn a_reset_user_joining_again_is_announced() {
    let mut w = world().await;
    // The admin resets Bob: his phone is removed from the group and revoked.
    let leaf = {
        let phone = &w.phone;
        let group = w.admin.group();
        group
            .members()
            .find(|m| m.signature_key == phone.credential_with_key().signature_key.as_slice())
            .check("Bob's leaf")
            .index
    };
    let epoch = w.admin.epoch();
    let commit = w.admin.remove(&[leaf]);
    w.admin_stream
        .accepted(commit_envelope(epoch, commit, None))
        .await;
    w.admin.confirm();
    receive(&mut w.laptop_stream, &mut w.laptop).await;
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

    // Bob registers again with a new identity key and a new phone.
    let invite = create_invite(&w.conn, &w.admin, "bob").await;
    let new_bob = TestUser::new("bob");
    let mut new_phone = new_bob.new_device();
    register(
        &w.conn,
        &invite.invite.check("invite").code,
        &new_bob,
        &new_phone,
    )
    .await
    .check("Register again");
    new_phone.authenticate(&w.conn).await;
    new_phone.upload_key_packages(&w.conn, 10).await;

    // The Pi announces it; the admin's device checks the changed key.
    names(&w.admin_stream.join_notice().await, &new_phone);
}

// spec: group-membership/automatic-add-of-new-users
#[tokio::test]
async fn two_admin_devices_race_and_the_loser_is_stale() {
    let mut w = world().await;
    // The admin's tablet is in the group too.
    let (mut tablet, mut tablet_stream) = admin_tablet(&mut w).await;

    let (carol, mut phone) = carol_registers(&w).await;
    phone.upload_key_packages(&w.conn, 10).await;
    names(&w.admin_stream.join_notice().await, &phone);
    names(&tablet_stream.join_notice().await, &phone);

    // Both commit the Add in the same epoch; the Pi accepts one.
    let mut carol_stream = Stream::open(&w.conn, &phone).await;
    let epoch = w.admin.epoch();
    let first = fetch_device(&w, &w.admin, &carol, &phone).await;
    let second = fetch_device(&w, &tablet, &carol, &phone).await;
    let (commit, welcome) = w.admin.add(&first);
    let seq = w
        .admin_stream
        .accepted(commit_envelope(epoch, commit, Some((welcome, &[&phone]))))
        .await;
    w.admin.confirm();
    let (commit, welcome) = tablet.add(&second);
    let rejected = tablet_stream
        .rejected(commit_envelope(epoch, commit, Some((welcome, &[&phone]))))
        .await;
    assert_eq!(rejected.reason, pb::RejectionReason::StaleEpoch as i32);
    tablet.abort();

    // Carol's phone receives only the winner's Welcome.
    let got = receive(&mut carol_stream, &mut phone).await;
    assert_eq!(got.kind, Kind::Welcome as i32);
    assert_eq!(got.server_seq, seq);
    carol_stream.sync().await;
    carol_stream.assert_quiet().await;
}

/// A second device of the admin, linked and added to the group by the
/// admin's phone, with its stream.
async fn admin_tablet(w: &mut World) -> (TestDevice, Stream) {
    let mut tablet = link_device(&w.conn, &w.admin_user, &w.admin).await;
    tablet.upload_key_packages(&w.conn, 10).await;
    let mut stream = Stream::open(&w.conn, &tablet).await;
    let key_packages = fetch_device(w, &w.admin, &w.admin_user, &tablet).await;
    add_over_stream(
        &mut w.admin,
        &mut w.admin_stream,
        &key_packages,
        &mut [
            (&mut w.laptop, &mut w.laptop_stream),
            (&mut w.phone, &mut w.phone_stream),
        ],
        &mut [(&mut tablet, &mut stream)],
    )
    .await;
    (tablet, stream)
}
