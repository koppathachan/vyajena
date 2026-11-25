//! group-membership: Inactive devices are kept out; Automatic re-add;
//! key-directory: Replenishment signal. Over `Messaging.Connect`, with the
//! manual clock standing in for a week in a drawer.

mod support;

use std::time::{Duration, SystemTime};

use server::membership::GroupState;
use support::clients::TestDevice;
use support::harness::{Conn, TestServer, authed};
use support::messaging::{
    Stream, World, add_over_stream, commit_envelope, offline, receive, reconnect, say,
    wait_offline, world,
};
use support::pb::envelope::Kind;
use support::pb::server_frame::Frame as Out;
use support::{Check, invite_user, link_device, pb};

const DAY: Duration = Duration::from_secs(24 * 3600);

async fn queued(server: &TestServer, device: &TestDevice) -> i64 {
    let id = device.device_id.to_vec();
    server
        .query(move |db| {
            db.query_row(
                "SELECT count(*) FROM deliveries WHERE device_id = ?1",
                [id],
                |r| r.get(0),
            )
        })
        .await
}

async fn is_member(server: &TestServer, device: &TestDevice) -> bool {
    let id = device.device_id;
    server
        .state
        .store
        .call(|db| GroupState::main_members(db))
        .await
        .check("members")
        .iter()
        .any(|m| m.device_id == id)
}

async fn inactive_count(server: &TestServer) -> i64 {
    server
        .query(|db| db.query_row("SELECT count(*) FROM inactive_devices", [], |r| r.get(0)))
        .await
}

/// Bob's phone goes offline right after the admin's message M; the admin,
/// the laptop and `others` keep connecting. 7 days and a minute later the
/// sweep marks the phone inactive. Returns M's `accepted_at`.
async fn phone_in_drawer(
    w: &mut World,
    others: &mut [(&mut TestDevice, &mut Stream)],
) -> SystemTime {
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;
    say(
        &mut w.admin,
        &mut w.admin_stream,
        b"M, which the phone misses",
    )
    .await;
    let m_at = w.server.clock_now();
    receive(&mut w.laptop_stream, &mut w.laptop).await;
    for (device, stream) in others.iter_mut() {
        receive(stream, device).await;
    }

    w.server.clock.advance(4 * DAY);
    w.admin_stream = reconnect(&w.conn, &mut w.admin).await;
    w.laptop_stream = reconnect(&w.conn, &mut w.laptop).await;
    for (device, stream) in others.iter_mut() {
        **stream = reconnect(&w.conn, device).await;
    }
    w.server.clock.advance(3 * DAY + Duration::from_secs(60));
    w.server.sweep().await;
    m_at
}

// spec: group-membership/inactive-devices-are-kept-out
#[tokio::test]
async fn phone_left_in_a_drawer_is_proposed_for_removal() {
    let mut w = world().await;
    let m_at = phone_in_drawer(&mut w, &mut []).await;

    // Inactive, its queue deleted, nothing more queued for it.
    assert_eq!(inactive_count(&w.server).await, 1);
    assert_eq!(queued(&w.server, &w.phone).await, 0);
    let missed: i64 = w
        .server
        .query(|db| {
            db.query_row("SELECT missed_since FROM inactive_devices", [], |r| {
                r.get(0)
            })
        })
        .await;
    assert_eq!(missed, server::runtime::clock::unix_millis(m_at));

    // The server's external Remove reaches the members, signed by no member.
    w.admin_stream = reconnect(&w.conn, &mut w.admin).await;
    let proposal = receive(&mut w.admin_stream, &mut w.admin).await;
    assert_eq!(proposal.kind, Kind::Proposal as i32);
    assert_eq!(proposal.sender, None);
    assert_eq!(proposal.epoch, w.admin.epoch());
    w.laptop_stream = reconnect(&w.conn, &mut w.laptop).await;
    let proposal = w.laptop_stream.envelope().await;
    assert_eq!(proposal.kind, Kind::Proposal as i32);

    // Until the Remove is committed, nothing is queued for the phone.
    say(&mut w.laptop, &mut w.laptop_stream, b"M2").await;
    assert_eq!(queued(&w.server, &w.phone).await, 0);
    receive(&mut w.admin_stream, &mut w.admin).await;

    // Any member device may commit it: here Alice's laptop.
    w.laptop.receive_proposal(&proposal.ciphertext);
    let epoch = w.laptop.epoch();
    let commit = w.laptop.commit_pending();
    w.laptop_stream
        .accepted(commit_envelope(epoch, commit, None))
        .await;
    w.laptop.confirm();
    assert!(!is_member(&w.server, &w.phone).await);
    receive(&mut w.admin_stream, &mut w.admin).await;
    assert_eq!(w.admin.epoch(), w.laptop.epoch());
}

// spec: group-membership/inactive-devices-are-kept-out
#[tokio::test]
async fn server_reissues_its_remove_in_each_new_epoch() {
    let mut w = world().await;
    phone_in_drawer(&mut w, &mut []).await;
    w.admin_stream = reconnect(&w.conn, &mut w.admin).await;
    w.laptop_stream = reconnect(&w.conn, &mut w.laptop).await;
    let first = w.admin_stream.envelope().await;
    assert_eq!(first.kind, Kind::Proposal as i32);
    let epoch = w.laptop.epoch();
    assert_eq!(first.epoch, epoch);

    // The laptop commits without it (it never processed the proposal).
    let laptop_proposal = w.laptop_stream.envelope().await;
    assert_eq!(laptop_proposal.server_seq, first.server_seq);
    let update = w.laptop.self_update();
    let commit_seq = w
        .laptop_stream
        .accepted(commit_envelope(epoch, update, None))
        .await;
    w.laptop.confirm();
    assert!(is_member(&w.server, &w.phone).await);

    // The server proposes again in the new epoch.
    let commit = receive(&mut w.admin_stream, &mut w.admin).await;
    assert_eq!(commit.server_seq, commit_seq);
    let again = receive(&mut w.admin_stream, &mut w.admin).await;
    assert_eq!(again.kind, Kind::Proposal as i32);
    assert_eq!(again.sender, None);
    assert_eq!(again.epoch, epoch + 1);
    assert!(again.server_seq > commit_seq);
    let commit = w.admin.commit_pending();
    w.admin_stream
        .accepted(commit_envelope(epoch + 1, commit, None))
        .await;
    w.admin.confirm();
    assert!(!is_member(&w.server, &w.phone).await);
}

async fn fetch_phone(
    conn: &Conn,
    caller: &TestDevice,
    w_bob: &support::clients::TestUser,
    phone: &TestDevice,
) -> Vec<Vec<u8>> {
    conn.keys()
        .fetch_key_packages(authed(
            caller.token(),
            pb::FetchKeyPackagesRequest {
                user_id: Some(w_bob.pb_user_id()),
                device_ids: vec![phone.device_id.to_vec()],
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

fn removed_since(frame: Out) -> SystemTime {
    match frame {
        Out::RemovedNotice(notice) => {
            assert_eq!(notice.group_id, support::messaging::GROUP_ID);
            SystemTime::try_from(notice.missed_since.check("missed_since")).check("time")
        }
        other => panic!("expected a removed notice, got {other:?}"),
    }
}

fn rejoin_for(frame: Out) -> pb::DeviceRef {
    match frame {
        Out::RejoinNotice(notice) => {
            assert_eq!(notice.group_id, support::messaging::GROUP_ID);
            notice.device.check("device")
        }
        other => panic!("expected a rejoin notice, got {other:?}"),
    }
}

// spec: group-membership/automatic-re-add
#[tokio::test]
async fn back_from_holiday_the_admin_re_adds_the_phone() {
    let mut w = world().await;
    let m_at = phone_in_drawer(&mut w, &mut []).await;
    w.server.clock.advance(3 * DAY);
    w.admin_stream = reconnect(&w.conn, &mut w.admin).await;
    receive(&mut w.admin_stream, &mut w.admin).await; // the server's Remove

    // Bob's phone comes back after 10 days.
    w.phone.authenticate(&w.conn).await;
    w.phone_stream = Stream::open(&w.conn, &w.phone).await;
    assert_eq!(removed_since(w.phone_stream.next().await), m_at);
    assert_eq!(
        rejoin_for(w.admin_stream.next().await),
        w.phone.device_ref()
    );

    // The admin's app re-adds it: KeyPackage for the phone only, Add plus the
    // pending Remove of its old leaf, Welcome in the same COMMIT.
    let key_packages = fetch_phone(&w.conn, &w.admin, &w.bob, &w.phone).await;
    assert_eq!(key_packages.len(), 1);
    let epoch = w.admin.epoch();
    let (commit, welcome) = w.admin.add(&key_packages);
    let seq = w
        .admin_stream
        .accepted(commit_envelope(epoch, commit, Some((welcome, &[&w.phone]))))
        .await;
    w.admin.confirm();

    // The phone continues from the Welcome; M is never delivered.
    let got = receive(&mut w.phone_stream, &mut w.phone).await;
    assert_eq!(got.kind, Kind::Welcome as i32);
    assert_eq!(got.server_seq, seq);
    assert_eq!(inactive_count(&w.server).await, 0);
    say(&mut w.admin, &mut w.admin_stream, b"welcome back").await;
    let got = w.phone_stream.envelope().await;
    assert_eq!(w.phone.read_message(&got.ciphertext), b"welcome back");
    assert!(is_member(&w.server, &w.phone).await);
}

// group-resync G3: the "removed for inactivity" notice is durable. A device
// offline when its leaf was committed-removed is still told, with the
// since-when, on its next connect — not only if it was online at removal.
// spec: group-membership/automatic-re-add
#[tokio::test]
async fn removed_while_offline_is_told_on_next_connect() {
    let mut w = world().await;
    let m_at = phone_in_drawer(&mut w, &mut []).await;

    // An active member commits the server's Remove while the phone is still
    // offline: its leaf is actually gone from the group.
    w.admin_stream = reconnect(&w.conn, &mut w.admin).await;
    let proposal = receive(&mut w.admin_stream, &mut w.admin).await;
    assert_eq!(proposal.kind, Kind::Proposal as i32);
    w.admin.receive_proposal(&proposal.ciphertext);
    let epoch = w.admin.epoch();
    let commit = w.admin.commit_pending();
    w.admin_stream
        .accepted(commit_envelope(epoch, commit, None))
        .await;
    w.admin.confirm();
    assert!(!is_member(&w.server, &w.phone).await);

    // The phone was offline the whole time. On its next connect the server
    // still delivers the removal notice with the since-when, so the app can
    // show it instead of leaving the user with no indication.
    w.phone.authenticate(&w.conn).await;
    w.phone_stream = Stream::open(&w.conn, &w.phone).await;
    assert_eq!(removed_since(w.phone_stream.next().await), m_at);
}

// spec: group-membership/automatic-re-add
#[tokio::test]
async fn two_devices_race_to_re_add_and_the_loser_is_stale() {
    let mut w = world().await;
    // Bob also has a tablet in the group.
    let mut tablet = link_device(&w.conn, &w.bob, &w.phone).await;
    tablet.upload_key_packages(&w.conn, 10).await;
    let mut tablet_stream = Stream::open(&w.conn, &tablet).await;
    let key_packages = fetch_phone(&w.conn, &w.phone, &w.bob, &tablet).await;
    add_over_stream(
        &mut w.phone,
        &mut w.phone_stream,
        &key_packages,
        &mut [
            (&mut w.admin, &mut w.admin_stream),
            (&mut w.laptop, &mut w.laptop_stream),
        ],
        &mut [(&mut tablet, &mut tablet_stream)],
    )
    .await;
    tablet_stream.sync().await;

    phone_in_drawer(&mut w, &mut [(&mut tablet, &mut tablet_stream)]).await;
    w.server.clock.advance(DAY);
    w.admin_stream = reconnect(&w.conn, &mut w.admin).await;
    tablet_stream = reconnect(&w.conn, &mut tablet).await;
    receive(&mut w.admin_stream, &mut w.admin).await; // the server's Remove
    receive(&mut tablet_stream, &mut tablet).await; // the server's Remove

    w.phone.authenticate(&w.conn).await;
    w.phone_stream = Stream::open(&w.conn, &w.phone).await;
    removed_since(w.phone_stream.next().await);
    assert_eq!(rejoin_for(tablet_stream.next().await), w.phone.device_ref());
    assert_eq!(
        rejoin_for(w.admin_stream.next().await),
        w.phone.device_ref()
    );

    // Both commit a re-add in the same epoch; the tablet's arrives first.
    let epoch = w.admin.epoch();
    let tablet_kp = fetch_phone(&w.conn, &tablet, &w.bob, &w.phone).await;
    let admin_kp = fetch_phone(&w.conn, &w.admin, &w.bob, &w.phone).await;
    let (tablet_commit, tablet_welcome) = tablet.add(&tablet_kp);
    let (admin_commit, admin_welcome) = w.admin.add(&admin_kp);
    tablet_stream
        .accepted(commit_envelope(
            epoch,
            tablet_commit,
            Some((tablet_welcome.clone(), &[&w.phone])),
        ))
        .await;
    tablet.confirm();
    let rejected = w
        .admin_stream
        .rejected(commit_envelope(
            epoch,
            admin_commit,
            Some((admin_welcome, &[&w.phone])),
        ))
        .await;
    assert_eq!(rejected.reason, pb::RejectionReason::StaleEpoch as i32);
    assert_eq!(rejected.current_epoch, epoch + 1);
    w.admin.abort();

    // The phone gets only the winner's Welcome.
    let got = receive(&mut w.phone_stream, &mut w.phone).await;
    assert_eq!(got.kind, Kind::Welcome as i32);
    assert_eq!(got.ciphertext, tablet_welcome);
    assert_eq!(got.sender, Some(tablet.device_ref()));
    w.phone_stream.assert_quiet().await;
    // The admin processes the winning commit and sees the phone is back.
    receive(&mut w.admin_stream, &mut w.admin).await;
    assert_eq!(w.admin.epoch(), epoch + 1);
    assert!(is_member(&w.server, &w.phone).await);
}

// spec: group-membership/automatic-re-add
#[tokio::test]
async fn nobody_able_to_re_add_is_online_so_it_waits() {
    let mut w = world().await;
    phone_in_drawer(&mut w, &mut []).await;
    w.server.clock.advance(DAY);
    // The admin's and laptop's streams idle out; nobody who can re-add is on.
    wait_offline(&w.server, &w.admin).await;
    wait_offline(&w.server, &w.laptop).await;
    w.laptop_stream = reconnect(&w.conn, &mut w.laptop).await;
    receive(&mut w.laptop_stream, &mut w.laptop).await; // the server's Remove

    w.phone.authenticate(&w.conn).await;
    w.phone_stream = Stream::open(&w.conn, &w.phone).await;
    removed_since(w.phone_stream.next().await);
    w.phone_stream.sync().await;
    w.phone_stream.assert_quiet().await;
    // Alice's laptop cannot re-add Bob's phone and hears nothing about it.
    w.laptop_stream.assert_quiet().await;

    // When the admin next connects it hears of the phone and re-adds it.
    w.admin.authenticate(&w.conn).await;
    w.admin_stream = Stream::open(&w.conn, &w.admin).await;
    assert_eq!(
        rejoin_for(w.admin_stream.next().await),
        w.phone.device_ref()
    );
    receive(&mut w.admin_stream, &mut w.admin).await; // the server's Remove
    let key_packages = fetch_phone(&w.conn, &w.admin, &w.bob, &w.phone).await;
    let epoch = w.admin.epoch();
    let (commit, welcome) = w.admin.add(&key_packages);
    w.admin_stream
        .accepted(commit_envelope(epoch, commit, Some((welcome, &[&w.phone]))))
        .await;
    w.admin.confirm();
    let got = receive(&mut w.phone_stream, &mut w.phone).await;
    assert_eq!(got.kind, Kind::Welcome as i32);
}

// spec: key-directory/replenishment-signal
#[tokio::test]
async fn low_water_mark_notice_when_the_count_drops_from_5_to_4() {
    let w = world().await;
    let (carol, phone) = invite_user(&w.conn, &w.admin, "carol").await;
    phone.upload_key_packages(&w.conn, 5).await;
    let mut stream = Stream::open(&w.conn, &phone).await;
    stream.sync().await;
    stream.assert_quiet().await;

    for remaining in [4, 3] {
        support::messaging::fetch(&w.conn, &w.admin, &carol).await;
        match stream.next().await {
            Out::LowKeyPackages(low) => assert_eq!(low.remaining, remaining),
            other => panic!("expected a low-KeyPackages notice, got {other:?}"),
        }
    }
}

// spec: key-directory/replenishment-signal
#[tokio::test]
async fn device_low_on_keypackages_is_told_when_it_connects() {
    let w = world().await;
    let (_, phone) = invite_user(&w.conn, &w.admin, "carol").await;
    phone.upload_key_packages(&w.conn, 2).await;
    let mut stream = Stream::open(&w.conn, &phone).await;
    match stream.next().await {
        Out::LowKeyPackages(low) => assert_eq!(low.remaining, 2),
        other => panic!("expected a low-KeyPackages notice, got {other:?}"),
    }
}
