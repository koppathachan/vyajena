//! direct-messages on the Pi (change `dms`): opening a DM, who may be in it,
//! old apps kept out, DM join notices, closing on user removal, and what the
//! Pi stores.
//!
//! Cast (support::messaging::World): the admin (`akhil`), Alice's laptop and
//! Bob's phone, all in the main group. Alice and Bob run a version 2 core
//! unless a test says otherwise.

mod support;

use std::time::Duration;
use support::Check;
use support::clients::TestDevice;
use support::harness::authed;

use support::messaging::{
    Stream, World, commit_envelope_in, envelope_in, offline, receive, reconnect, wait_offline,
    world,
};
use support::pb;
use support::pb::envelope::Kind;
use support::pb::server_frame::Frame as Out;
use support::pi_external_sender;
use tonic::Code;

const DM_ID: &[u8] = b"alice-and-bob-dm";

fn dm_users() -> (&'static str, &'static str) {
    ("alice@pi", "bob@pi")
}

/// Alice's laptop opens the Alice–Bob DM (its main group moves to `other`).
async fn open_dm(w: &mut World) -> pb::CreateDmResponse {
    let pi = pi_external_sender(&w.conn).await;
    let request = w
        .laptop
        .create_dm(DM_ID, &pi, dm_users(), w.bob.pb_user_id());
    w.conn
        .dms()
        .create_dm(authed(w.laptop.token(), request))
        .await
        .check("CreateDm")
        .into_inner()
}

/// The World with Alice's laptop and Bob's phone on version 2 KeyPackages.
async fn v2_world() -> World {
    let w = world().await;
    w.laptop.upload_dm_key_packages(&w.conn, 10).await;
    w.phone.upload_dm_key_packages(&w.conn, 10).await;
    w
}

/// Fetches KeyPackages of `user`'s devices for the DM.
async fn fetch_for_dm(
    w: &World,
    caller: &TestDevice,
    user: &support::clients::TestUser,
) -> Result<Vec<pb::DeviceKeyPackage>, tonic::Status> {
    w.conn
        .keys()
        .fetch_key_packages(authed(
            caller.token(),
            pb::FetchKeyPackagesRequest {
                user_id: Some(user.pb_user_id()),
                device_ids: vec![],
                group_id: DM_ID.to_vec(),
            },
        ))
        .await
        .map(|r| r.into_inner().devices)
}

/// Alice's laptop adds Bob's phone to the DM; Bob's phone joins from its
/// Welcome (its main group moves to `other`).
async fn add_bob_to_dm(w: &mut World) {
    let key_packages: Vec<Vec<u8>> = fetch_for_dm(w, &w.laptop, &w.bob)
        .await
        .check("fetch for the DM")
        .into_iter()
        .map(|d| d.key_package)
        .collect();
    assert_eq!(key_packages.len(), 1);
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
    assert_eq!(got.group_id, DM_ID);
    w.phone.swap_groups();
    w.phone.join(&got.ciphertext);
    w.phone_stream.ack_in(DM_ID, got.server_seq).await;
}

// spec: direct-messages/opening-a-dm
#[tokio::test]
async fn alice_opens_a_dm_with_bob() {
    let mut w = v2_world().await;
    let response = open_dm(&mut w).await;
    assert!(response.created);
    assert_eq!(response.group_id, DM_ID);
    let row: (String, i64, String, String) =
        w.pi.query(|db| {
            db.query_row(
                "SELECT kind, epoch, dm_user_a, dm_user_b FROM mls_groups WHERE group_id = ?1",
                [DM_ID],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
        })
        .await;
    assert_eq!(row, ("dm".into(), 0, "alice".into(), "bob".into()));
    let members: i64 =
        w.pi.query(|db| {
            db.query_row(
                "SELECT count(*) FROM mls_members WHERE group_id = ?1",
                [DM_ID],
                |r| r.get(0),
            )
        })
        .await;
    assert_eq!(members, 1);
}

// spec: direct-messages/opening-a-dm
#[tokio::test]
async fn both_open_at_once_and_the_second_gets_the_first() {
    let mut w = v2_world().await;
    open_dm(&mut w).await;
    let pi = pi_external_sender(&w.conn).await;
    let request = w
        .phone
        .create_dm(b"bobs-own-dm-try!", &pi, dm_users(), w.alice.pb_user_id());
    let response = w
        .conn
        .dms()
        .create_dm(authed(w.phone.token(), request))
        .await
        .check("second CreateDm")
        .into_inner();
    assert!(!response.created);
    assert_eq!(response.group_id, DM_ID);
    let groups: i64 =
        w.pi.query(|db| db.query_row("SELECT count(*) FROM mls_groups", [], |r| r.get(0)))
            .await;
    assert_eq!(groups, 2, "the main group and one DM");
}

// spec: direct-messages/opening-a-dm
#[tokio::test]
async fn a_dm_needs_another_user_and_the_right_extension() {
    let mut w = v2_world().await;
    let pi = pi_external_sender(&w.conn).await;

    let to_self = w
        .laptop
        .create_dm(DM_ID, &pi, dm_users(), w.alice.pb_user_id());
    let status = w
        .conn
        .dms()
        .create_dm(authed(w.laptop.token(), to_self))
        .await
        .err()
        .check("a DM with yourself");
    assert_eq!(status.code(), Code::InvalidArgument);

    let carol = w.laptop.create_dm(
        b"dm-naming-carol!",
        &pi,
        ("alice@pi", "carol@pi"),
        w.bob.pb_user_id(),
    );
    let status = w
        .conn
        .dms()
        .create_dm(authed(w.laptop.token(), carol))
        .await
        .err()
        .check("an extension naming someone else");
    assert_eq!(status.code(), Code::InvalidArgument);

    let unknown = w.laptop.create_dm(
        b"dm-with-nobody!!",
        &pi,
        ("alice@pi", "zed@pi"),
        pb::UserId {
            username: "zed".into(),
            server_id: "pi".into(),
        },
    );
    let status = w
        .conn
        .dms()
        .create_dm(authed(w.laptop.token(), unknown))
        .await
        .err()
        .check("an unknown peer");
    assert_eq!(status.code(), Code::NotFound);

    let groups: i64 =
        w.pi.query(|db| db.query_row("SELECT count(*) FROM mls_groups", [], |r| r.get(0)))
            .await;
    assert_eq!(groups, 1, "nothing recorded");
}

// spec: direct-messages/old-apps-never-join-a-dm
#[tokio::test]
async fn a_device_with_only_old_key_packages_cannot_open_a_dm() {
    let mut w = world().await;
    let pi = pi_external_sender(&w.conn).await;
    let request = w
        .laptop
        .create_dm(DM_ID, &pi, dm_users(), w.bob.pb_user_id());
    let status = w
        .conn
        .dms()
        .create_dm(authed(w.laptop.token(), request))
        .await
        .err()
        .check("CreateDm from a 1.0.0 core's KeyPackages");
    assert_eq!(status.code(), Code::FailedPrecondition);
}

// spec: direct-messages/old-apps-never-join-a-dm
// spec: key-directory/keypackage-fetch
#[tokio::test]
async fn bobs_old_app_is_left_out_until_it_updates() {
    let mut w = world().await;
    w.laptop.upload_dm_key_packages(&w.conn, 10).await;
    open_dm(&mut w).await;
    w.laptop_stream.forget_join_notices();

    // Bob's phone still runs 1.0.0: nothing of it for the DM.
    let devices = fetch_for_dm(&w, &w.laptop, &w.bob).await.check("fetch");
    assert!(devices.is_empty());
    w.laptop_stream.assert_no_join_notice().await;

    // Bob updates: his core uploads KeyPackages that list 0xF0A2.
    w.phone.upload_dm_key_packages(&w.conn, 10).await;
    let notice = w.laptop_stream.join_notice().await;
    assert_eq!(notice.group_id, DM_ID);
    assert_eq!(
        notice.device.check("device").device_id,
        w.phone.device_id.to_vec()
    );
    add_bob_to_dm(&mut w).await;
}

// spec: key-directory/keypackage-fetch
#[tokio::test]
async fn only_a_member_fetches_for_the_dm_and_only_for_its_users() {
    let mut w = v2_world().await;
    open_dm(&mut w).await;
    // The admin is not in the DM.
    let status = fetch_for_dm(&w, &w.admin, &w.bob)
        .await
        .err()
        .check("the admin fetching for the DM");
    assert_eq!(status.code(), Code::PermissionDenied);
    // Alice is, but the admin is not one of its users.
    let status = fetch_for_dm(&w, &w.laptop, &w.admin_user)
        .await
        .err()
        .check("fetching the admin for the DM");
    assert_eq!(status.code(), Code::PermissionDenied);
    // Bob's KeyPackages were not used up.
    let left = fetch_for_dm(&w, &w.laptop, &w.bob).await.check("fetch");
    assert_eq!(left.len(), 1);
}

// spec: direct-messages/dm-members-kept-complete
#[tokio::test]
async fn the_creator_hears_which_devices_the_new_dm_needs() {
    let mut w = v2_world().await;
    w.laptop_stream.forget_join_notices();
    open_dm(&mut w).await;
    let notice = w.laptop_stream.join_notice().await;
    assert_eq!(notice.group_id, DM_ID);
    assert_eq!(
        notice.device.check("device").device_id,
        w.phone.device_id.to_vec()
    );
    add_bob_to_dm(&mut w).await;

    // Once Bob's phone is in, a reconnect brings no more notices.
    w.laptop_stream = reconnect(&w.conn, &mut w.laptop).await;
    w.laptop_stream.assert_no_join_notice().await;
}

// spec: direct-messages/dm-members
// spec: group-membership/commit-authority
#[tokio::test]
async fn the_dm_holds_only_its_two_users() {
    let mut w = v2_world().await;
    open_dm(&mut w).await;
    add_bob_to_dm(&mut w).await;

    // Alice adds the admin's device, a third user, even with a KeyPackage
    // that lists 0xF0A2.
    let epoch = w.laptop.epoch();
    let admin_dm_key_package = w.admin.dm_key_package();
    let (commit, welcome) = w.laptop.add(&[admin_dm_key_package]);
    let rejected = w
        .laptop_stream
        .rejected(commit_envelope_in(
            DM_ID,
            epoch,
            commit,
            Some((welcome, &[&w.admin])),
        ))
        .await;
    assert_eq!(rejected.reason, pb::RejectionReason::PolicyViolation as i32);
    w.laptop.abort();

    // Alice removes Bob's phone: only Bob may remove his devices.
    let bobs_leaf = w.phone.leaf();
    let commit = w.laptop.remove(&[bobs_leaf]);
    let rejected = w
        .laptop_stream
        .rejected(commit_envelope_in(DM_ID, epoch, commit, None))
        .await;
    assert_eq!(rejected.reason, pb::RejectionReason::PolicyViolation as i32);
    w.laptop.abort();
}

// spec: direct-messages/dm-content-stays-encrypted
// spec: group-membership/membership-state-on-the-pi
#[tokio::test]
async fn dm_messages_reach_only_the_dm() {
    let mut w = v2_world().await;
    open_dm(&mut w).await;
    add_bob_to_dm(&mut w).await;
    w.admin_stream.sync().await;

    let epoch = w.laptop.epoch();
    let ciphertext = w.laptop.message(b"just between us");
    let seq = w
        .laptop_stream
        .accepted(envelope_in(DM_ID, Kind::Application, epoch, ciphertext))
        .await;
    let got = receive(&mut w.phone_stream, &mut w.phone).await;
    assert_eq!(got.server_seq, seq);
    assert_eq!(got.group_id, DM_ID);
    w.admin_stream.assert_quiet().await;

    // What the Pi keeps for the DM: no text, no names.
    let stored: Vec<Vec<u8>> =
        w.pi.query(|db| {
            let mut stmt = db.prepare("SELECT ciphertext FROM envelopes")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect()
        })
        .await;
    assert!(
        stored
            .iter()
            .all(|c| !c.windows(15).any(|w| w == b"just between us"))
    );
}

// spec: direct-messages/dm-closed-on-user-removal
#[tokio::test]
async fn removing_bob_closes_the_dm() {
    let mut w = v2_world().await;
    open_dm(&mut w).await;
    add_bob_to_dm(&mut w).await;

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

    // Alice's laptop hears it, and keeps hearing it until it acknowledges.
    let closed = next_dm_closed(&mut w.laptop_stream).await;
    assert_eq!(closed.group_id, DM_ID);
    assert!(closed.closed_at.is_some());
    w.laptop_stream = reconnect(&w.conn, &mut w.laptop).await;
    let again = next_dm_closed(&mut w.laptop_stream).await;
    assert_eq!(again.group_id, DM_ID);
    w.laptop_stream.ack_in(DM_ID, 0).await;
    w.laptop_stream.sync().await;
    w.laptop_stream = reconnect(&w.conn, &mut w.laptop).await;
    w.laptop_stream.assert_quiet().await;

    // Nothing more goes into it.
    let epoch = w.laptop.epoch();
    let ciphertext = w.laptop.message(b"still there?");
    let rejected = w
        .laptop_stream
        .rejected(envelope_in(DM_ID, Kind::Application, epoch, ciphertext))
        .await;
    assert_eq!(rejected.reason, pb::RejectionReason::GroupClosed as i32);

    // The Pi kept only the row, closed.
    let (closed_at, members, state): (Option<i64>, i64, i64) =
        w.pi.query(|db| {
            db.query_row(
                "SELECT closed_at,
                        (SELECT count(*) FROM mls_members WHERE group_id = ?1),
                        (SELECT count(*) FROM mls_public_state WHERE group_id = ?1)
                 FROM mls_groups WHERE group_id = ?1",
                [DM_ID],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
        })
        .await;
    assert!(closed_at.is_some());
    assert_eq!((members, state), (0, 0));
}

// spec: direct-messages/dm-closed-on-user-removal
#[tokio::test]
async fn the_admin_has_no_call_on_other_peoples_dms() {
    let mut w = v2_world().await;
    open_dm(&mut w).await;
    add_bob_to_dm(&mut w).await;
    let admin_in_dm: i64 =
        w.pi.query(move |db| {
            db.query_row(
                "SELECT count(*) FROM mls_members m JOIN devices d ON d.device_id = m.device_id
                 WHERE m.group_id = ?1 AND d.username = 'akhil'",
                [DM_ID],
                |r| r.get(0),
            )
        })
        .await;
    assert_eq!(admin_in_dm, 0);
    // The admin's device cannot write into it either.
    let rejected = w
        .admin_stream
        .rejected(envelope_in(DM_ID, Kind::Application, 1, vec![0; 8]))
        .await;
    assert_ne!(rejected.reason, 0);
}

/// The next `DmClosed` frame; other frames before it are skipped.
async fn next_dm_closed(stream: &mut Stream) -> pb::DmClosed {
    loop {
        if let Out::DmClosed(closed) = stream.next().await {
            return closed;
        }
    }
}

const DAY: Duration = Duration::from_secs(24 * 3600);

// spec: direct-messages/inactivity-across-groups
#[tokio::test]
async fn an_inactive_phone_leaves_every_group_and_hears_of_each() {
    let mut w = v2_world().await;
    open_dm(&mut w).await;
    add_bob_to_dm(&mut w).await;

    // Bob's phone lies in a drawer for a week; the others keep connecting.
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.pi, &w.phone).await;
    w.pi.clock.advance(4 * DAY);
    w.admin_stream = reconnect(&w.conn, &mut w.admin).await;
    w.laptop_stream = reconnect(&w.conn, &mut w.laptop).await;
    w.pi.clock.advance(3 * DAY + Duration::from_secs(60));
    w.pi.sweep().await;

    // The Pi proposed removing it in both groups.
    let phone = w.phone.device_id.to_vec();
    let groups: Vec<Vec<u8>> =
        w.pi.query(move |db| {
            let mut stmt =
                db.prepare("SELECT group_id FROM pi_removes WHERE device_id = ?1 ORDER BY 1")?;
            let rows = stmt.query_map([phone], |r| r.get(0))?;
            rows.collect()
        })
        .await;
    assert_eq!(groups.len(), 2, "main group and DM");
    assert!(groups.iter().any(|g| g == DM_ID));
    // Alice's laptop, in both, got a PROPOSAL in the DM.
    let mut saw_dm_proposal = false;
    for _ in 0..4 {
        let got = w.laptop_stream.envelope().await;
        if got.kind == Kind::Proposal as i32 && got.group_id == DM_ID {
            saw_dm_proposal = true;
            break;
        }
        w.laptop_stream.ack_in(&got.group_id, got.server_seq).await;
    }
    assert!(saw_dm_proposal);

    // Bob's phone comes back: it hears of its removal from each group.
    w.phone.authenticate(&w.conn).await;
    let mut stream = Stream::open(&w.conn, &w.phone).await;
    let mut removed = Vec::new();
    while removed.len() < 2 {
        if let Out::RemovedNotice(notice) = stream.next().await {
            removed.push(notice.group_id);
        }
    }
    assert!(removed.iter().any(|g| g == DM_ID));
    assert!(removed.iter().any(|g| g == support::messaging::GROUP_ID));
}

// spec: group-membership/group-creation
#[tokio::test]
async fn a_dm_is_not_a_second_main_group() {
    let mut w = v2_world().await;
    open_dm(&mut w).await;
    let pi = pi_external_sender(&w.conn).await;
    let mut again = TestDevice::new("akhil", &w.admin_user.identity);
    let request = again.create_group(b"another-main-grp", &pi, "akhil@pi");
    let status = w
        .conn
        .admin()
        .create_group(authed(w.admin.token(), request))
        .await
        .err()
        .check("a second CreateGroup");
    assert_eq!(status.code(), Code::AlreadyExists);
    let mains: i64 =
        w.pi.query(|db| {
            db.query_row(
                "SELECT count(*) FROM mls_groups WHERE kind = 'main'",
                [],
                |r| r.get(0),
            )
        })
        .await;
    assert_eq!(mains, 1);
}
