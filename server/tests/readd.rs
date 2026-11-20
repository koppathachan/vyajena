//! group-membership: Re-add after an unusable Welcome, the server's side
//! (welcome-recovery design D2-D4): who may ask with `Keys.RequestReadd`,
//! what the server changes on `READD_OUTCOME_ASKED`, and the daily limit. The
//! core asking and re-adding end to end is tested in client/tests.

mod support;

use std::time::Duration;

use protocol::group_membership::MAX_READD_REQUESTS_PER_DAY;
use support::clients::{TestDevice, TestUser};
use support::harness::{Conn, TestServer, authed};
use support::messaging::{Stream, World, commit_envelope, receive, say, world};
use support::pb::envelope::Kind;
use support::pb::server_frame::Frame as Out;
use support::{Check, create_invite, pb, register};

const DAY: Duration = Duration::from_secs(24 * 3600);
const REASON: &str = "a Welcome could not be used: no matching KeyPackage";

/// `device` asks to be added again with 10 fresh one-time
/// KeyPackages; returns the request's KeyPackages and the outcome.
async fn ask(
    conn: &Conn,
    device: &TestDevice,
    refused_welcome_seq: u64,
) -> (Vec<Vec<u8>>, pb::ReaddOutcome) {
    let key_packages: Vec<Vec<u8>> = (0..10).map(|_| device.key_package()).collect();
    let last_resort = device.key_package();
    let response = conn
        .keys()
        .request_readd(authed(
            device.token(),
            pb::RequestReaddRequest {
                refused_welcome_seq,
                reason: REASON.into(),
                key_packages: key_packages.clone(),
                last_resort_key_package: last_resort.clone(),
            },
        ))
        .await
        .check("RequestReadd")
        .into_inner();
    let mut all = key_packages;
    all.push(last_resort);
    (all, response.outcome())
}

async fn raw_ask(
    conn: &Conn,
    device: &TestDevice,
    key_packages: Vec<Vec<u8>>,
    reason: String,
) -> Result<tonic::Response<pb::RequestReaddResponse>, tonic::Status> {
    conn.keys()
        .request_readd(authed(
            device.token(),
            pb::RequestReaddRequest {
                refused_welcome_seq: 0,
                reason,
                key_packages,
                last_resort_key_package: device.key_package(),
            },
        ))
        .await
}

async fn stored_key_packages(server: &TestServer, device: &TestDevice) -> Vec<Vec<u8>> {
    let id = device.device_id.to_vec();
    server
        .query(move |db| {
            let mut stmt = db
                .prepare("SELECT key_package FROM key_packages WHERE device_id = ?1 ORDER BY id")?;
            let rows = stmt.query_map([id], |r| r.get(0))?;
            rows.collect()
        })
        .await
}

async fn inactive_count(server: &TestServer) -> i64 {
    server
        .query(|db| db.query_row("SELECT count(*) FROM inactive_devices", [], |r| r.get(0)))
        .await
}

async fn fetch_one(
    conn: &Conn,
    caller: &TestDevice,
    user: &TestUser,
    device: &TestDevice,
) -> Vec<u8> {
    let mut devices = conn
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
        .devices;
    assert_eq!(devices.len(), 1);
    devices.remove(0).key_package
}

/// The next two frames on the admin's stream after an ask: the server's external
/// Remove of the phone's leaf (applied to the admin's group) and the
/// `RejoinNotice` for the phone, in either order.
async fn admin_hears(w: &mut World) -> pb::DeviceRef {
    let mut rejoin = None;
    let mut proposal = false;
    while rejoin.is_none() || !proposal {
        match w.admin_stream.next().await {
            Out::RejoinNotice(notice) => {
                assert_eq!(notice.group_id, support::messaging::GROUP_ID);
                rejoin = notice.device;
            }
            Out::Envelope(envelope) => {
                assert_eq!(envelope.kind, Kind::Proposal as i32);
                assert_eq!(envelope.sender, None);
                w.admin.receive_proposal(&envelope.ciphertext);
                w.admin_stream.ack(envelope.server_seq).await;
                proposal = true;
            }
            other => panic!("expected a proposal and a rejoin notice, got {other:?}"),
        }
    }
    rejoin.check("rejoin device")
}

/// The admin's device re-adds Bob's phone after an ask: Remove (the server's
/// pending proposal) + Add from one of `asked`, the Welcome in the same
/// COMMIT; the phone joins from it. Returns the commit's `server_seq`.
async fn admin_readds_phone(w: &mut World, asked: &[Vec<u8>]) -> u64 {
    assert_eq!(admin_hears(w).await, w.phone.device_ref());
    let key_package = fetch_one(&w.conn, &w.admin, &w.bob, &w.phone).await;
    assert!(
        asked.contains(&key_package),
        "the re-add uses a new KeyPackage"
    );
    let epoch = w.admin.epoch();
    let (commit, welcome) = w.admin.add(&[key_package]);
    let seq = w
        .admin_stream
        .accepted(commit_envelope(epoch, commit, Some((welcome, &[&w.phone]))))
        .await;
    w.admin.confirm();
    receive(&mut w.laptop_stream, &mut w.laptop).await;
    loop {
        let got = w.phone_stream.envelope().await;
        w.phone_stream.ack(got.server_seq).await;
        if got.kind == Kind::Welcome as i32 {
            assert_eq!(got.server_seq, seq);
            w.phone.join(&got.ciphertext);
            break;
        }
    }
    assert_eq!(inactive_count(&w.server).await, 0);
    seq
}

/// Bob's phone has read everything queued for it (as a core that refused
/// its Welcome has, acking it all).
async fn phone_catches_up(w: &mut World) {
    w.phone_stream.sync().await;
}

// spec: group-membership/re-add-after-an-unusable-welcome
#[tokio::test]
async fn bobs_phone_refuses_its_welcome_and_is_re_added() {
    let mut w = world().await;
    phone_catches_up(&mut w).await;
    let before = stored_key_packages(&w.server, &w.phone).await;
    let (asked, outcome) = ask(&w.conn, &w.phone, 1).await;
    assert_eq!(outcome, pb::ReaddOutcome::Asked);

    // All of the phone's KeyPackages were replaced by the new ones.
    let stored = stored_key_packages(&w.server, &w.phone).await;
    assert_eq!(stored, asked);
    assert!(before.iter().all(|kp| !stored.contains(kp)));
    assert_eq!(inactive_count(&w.server).await, 1);

    admin_readds_phone(&mut w, &asked).await;
    // The phone follows the group again.
    say(&mut w.admin, &mut w.admin_stream, b"welcome, again").await;
    let got = w.phone_stream.envelope().await;
    assert_eq!(w.phone.read_message(&got.ciphertext), b"welcome, again");
}

// spec: group-membership/re-add-after-an-unusable-welcome
#[tokio::test]
async fn stuck_from_an_older_build_asks_with_seq_zero() {
    let mut w = world().await;
    phone_catches_up(&mut w).await;
    let (asked, outcome) = ask(&w.conn, &w.phone, 0).await;
    assert_eq!(outcome, pb::ReaddOutcome::Asked);
    admin_readds_phone(&mut w, &asked).await;
}

// spec: group-membership/re-add-after-an-unusable-welcome
#[tokio::test]
async fn still_waiting_for_the_first_add_changes_nothing() {
    let w = world().await;
    let invite = create_invite(&w.conn, &w.admin, "carol").await;
    let carol = TestUser::new("carol");
    let mut phone = carol.new_device();
    register(&w.conn, &invite.invite.check("invite").code, &carol, &phone)
        .await
        .check("Register");
    phone.authenticate(&w.conn).await;
    phone.upload_key_packages(&w.conn, 5).await;
    let before = stored_key_packages(&w.server, &phone).await;

    let (_, outcome) = ask(&w.conn, &phone, 0).await;
    assert_eq!(outcome, pb::ReaddOutcome::NotAMember);
    assert_eq!(stored_key_packages(&w.server, &phone).await, before);
    assert_eq!(inactive_count(&w.server).await, 0);
}

// spec: group-membership/re-add-after-an-unusable-welcome
#[tokio::test]
async fn welcome_on_its_way_changes_nothing_but_the_refused_one_does_not_count() {
    let mut w = world().await;
    // The admin adds Carol's phone; Carol's phone has not read the Welcome.
    let invite = create_invite(&w.conn, &w.admin, "carol").await;
    let carol = TestUser::new("carol");
    let mut phone = carol.new_device();
    register(&w.conn, &invite.invite.check("invite").code, &carol, &phone)
        .await
        .check("Register");
    phone.authenticate(&w.conn).await;
    phone.upload_key_packages(&w.conn, 5).await;
    let key_package = fetch_one(&w.conn, &w.admin, &carol, &phone).await;
    let epoch = w.admin.epoch();
    let (commit, welcome) = w.admin.add(&[key_package]);
    let seq = w
        .admin_stream
        .accepted(commit_envelope(epoch, commit, Some((welcome, &[&phone]))))
        .await;
    w.admin.confirm();
    let before = stored_key_packages(&w.server, &phone).await;

    // At start (seq 0) the queued Welcome comes first.
    let (_, outcome) = ask(&w.conn, &phone, 0).await;
    assert_eq!(outcome, pb::ReaddOutcome::WelcomeQueued);
    assert_eq!(stored_key_packages(&w.server, &phone).await, before);
    assert_eq!(inactive_count(&w.server).await, 0);

    // The core refused that Welcome; its ACK may still be on the way.
    let (asked, outcome) = ask(&w.conn, &phone, seq).await;
    assert_eq!(outcome, pb::ReaddOutcome::Asked);
    assert_eq!(stored_key_packages(&w.server, &phone).await, asked);
    // And asking again while waiting changes nothing.
    let (_, outcome) = ask(&w.conn, &phone, seq).await;
    assert_eq!(outcome, pb::ReaddOutcome::AlreadyWaiting);
    assert_eq!(stored_key_packages(&w.server, &phone).await, asked);
    // The admin's device hears about Carol's phone (it may re-add it).
    let mut stream = Stream::open(&w.conn, &w.admin).await;
    loop {
        if let Out::RejoinNotice(notice) = stream.next().await {
            assert_eq!(notice.device, Some(phone.device_ref()));
            break;
        }
    }
}

// spec: group-membership/re-add-after-an-unusable-welcome
#[tokio::test]
async fn welcome_refused_again_and_again_hits_the_daily_limit() {
    let mut w = world().await;
    for _ in 0..MAX_READD_REQUESTS_PER_DAY {
        phone_catches_up(&mut w).await;
        let (asked, outcome) = ask(&w.conn, &w.phone, 0).await;
        assert_eq!(outcome, pb::ReaddOutcome::Asked);
        admin_readds_phone(&mut w, &asked).await;
        w.server.clock.advance(Duration::from_secs(60));
    }
    phone_catches_up(&mut w).await;
    let before = stored_key_packages(&w.server, &w.phone).await;
    let (_, outcome) = ask(&w.conn, &w.phone, 0).await;
    assert_eq!(outcome, pb::ReaddOutcome::LimitReached);
    assert_eq!(stored_key_packages(&w.server, &w.phone).await, before);
    assert_eq!(inactive_count(&w.server).await, 0);

    // A day after the first ask, it may ask again.
    w.server.clock.advance(DAY);
    w.phone.authenticate(&w.conn).await;
    let (_, outcome) = ask(&w.conn, &w.phone, 0).await;
    assert_eq!(outcome, pb::ReaddOutcome::Asked);
}

// spec: group-membership/re-add-after-an-unusable-welcome
#[tokio::test]
async fn bad_key_packages_or_reason_fail_the_ask_and_change_nothing() {
    let w = world().await;
    let before = stored_key_packages(&w.server, &w.phone).await;
    let alice_kp = w.laptop.key_package();
    let status = raw_ask(&w.conn, &w.phone, vec![alice_kp], REASON.into())
        .await
        .expect_err("someone else's KeyPackage");
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
    let status = raw_ask(&w.conn, &w.phone, Vec::new(), "x".repeat(513))
        .await
        .expect_err("a long reason");
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
    assert_eq!(stored_key_packages(&w.server, &w.phone).await, before);
    assert_eq!(inactive_count(&w.server).await, 0);
}
