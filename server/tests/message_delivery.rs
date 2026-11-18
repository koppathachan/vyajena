//! message-delivery: every server-enforced requirement, over
//! `Messaging.Connect` with real OpenMLS members (pi-runtime: Durable storage
//! too, for the power cut).

mod support;

use std::time::Duration;

use protocol::device_identity::DeviceId;
use rusqlite::params;
use support::clients::{TestDevice, TestUser};
use support::harness::TestPi;
use support::messaging::{
    GROUP_ID, Stream, World, add_over_stream, commit_envelope, envelope, fetch, receive, say, world,
};
use support::pb::envelope::Kind;
use support::pb::server_frame::Frame as Out;
use support::{Check, invite_user, pb};

fn reason(rejected: &pb::send_result::Rejected) -> pb::RejectionReason {
    pb::RejectionReason::try_from(rejected.reason).check("reason")
}

/// The last `server_seq` handed out: unchanged means nothing was stored.
async fn last_seq(pi: &TestPi) -> i64 {
    pi.query(|db| db.query_row("SELECT last_server_seq FROM sequence", [], |r| r.get(0)))
        .await
}

/// Deliveries waiting for `device`.
async fn queued(pi: &TestPi, device: &TestDevice) -> Vec<u64> {
    let id = device.device_id.to_vec();
    pi.query(move |db| {
        let mut stmt = db.prepare(
            "SELECT server_seq FROM deliveries WHERE device_id = ?1 ORDER BY server_seq",
        )?;
        let rows = stmt.query_map(params![id], |r| r.get::<_, i64>(0))?;
        rows.map(|r| r.map(|n| n as u64)).collect()
    })
    .await
}

/// Carol, invited with one phone that has KeyPackages and an open stream.
async fn carol(w: &World) -> (TestUser, TestDevice, Stream) {
    let (carol, phone) = invite_user(&w.conn, &w.admin, "carol").await;
    phone.upload_key_packages(&w.conn, 10).await;
    let stream = Stream::open(&w.conn, &phone).await;
    (carol, phone, stream)
}

// spec: message-delivery/envelope
#[tokio::test]
async fn sender_cannot_be_spoofed_and_the_pi_sets_seq_and_time() {
    let mut w = world().await;
    let epoch = w.admin.epoch();
    let mut forged = envelope(Kind::Application, epoch, w.admin.message(b"from akhil"));
    forged.sender = Some(w.phone.device_ref());
    forged.server_seq = 9_999;
    forged.accepted_at = Some(prost_types::Timestamp {
        seconds: 1,
        nanos: 0,
    });
    let msg_id = forged.msg_id.clone();
    let seq = w.admin_stream.accepted(forged).await;
    assert_ne!(seq, 9_999);

    let got = w.laptop_stream.envelope().await;
    assert_eq!(got.sender, Some(w.admin.device_ref()));
    assert_eq!(got.server_seq, seq);
    assert_eq!(got.msg_id, msg_id);
    assert_eq!(got.group_id, GROUP_ID);
    assert_eq!(got.epoch, epoch);
    assert_eq!(got.kind, Kind::Application as i32);
    assert_eq!(
        got.accepted_at,
        Some(prost_types::Timestamp::from(w.pi.clock_now()))
    );
    assert_eq!(w.laptop.read_message(&got.ciphertext), b"from akhil");
}

// spec: message-delivery/framing-matches-the-envelope
#[tokio::test]
async fn mismatched_epoch_is_malformed() {
    let mut w = world().await;
    // Move to epoch 2 so the envelope's epoch is the current one.
    let epoch = w.admin.epoch();
    let commit = w.admin.self_update();
    w.admin_stream
        .accepted(commit_envelope(epoch, commit, None))
        .await;
    w.admin.confirm();
    receive(&mut w.laptop_stream, &mut w.laptop).await;
    let current = w.admin.epoch();

    // An MLS message from the previous epoch in an envelope that says current.
    let old = w.phone.message(b"sent before the commit");
    assert_eq!(w.phone.epoch(), current - 1);
    let rejected = w
        .phone_stream
        .rejected(envelope(Kind::Application, current, old))
        .await;
    assert_eq!(reason(&rejected), pb::RejectionReason::Malformed);
}

// spec: message-delivery/framing-matches-the-envelope
#[tokio::test]
async fn encrypted_commit_wrong_kind_and_wrong_group_are_malformed() {
    let mut w = world().await;
    let before = last_seq(&w.pi).await;
    let epoch = w.admin.epoch();
    // A PrivateMessage in a COMMIT envelope.
    let private = w.admin.message(b"not a commit");
    let rejected = w
        .admin_stream
        .rejected(envelope(Kind::Commit, epoch, private.clone()))
        .await;
    assert_eq!(reason(&rejected), pb::RejectionReason::Malformed);
    // A PrivateMessage in a PROPOSAL envelope.
    let rejected = w
        .admin_stream
        .rejected(envelope(Kind::Proposal, epoch, private.clone()))
        .await;
    assert_eq!(reason(&rejected), pb::RejectionReason::Malformed);
    // A PublicMessage commit in an APPLICATION envelope.
    let commit = w.admin.self_update();
    let rejected = w
        .admin_stream
        .rejected(envelope(Kind::Application, epoch, commit))
        .await;
    assert_eq!(reason(&rejected), pb::RejectionReason::Malformed);
    w.admin.abort();
    // The right message for another group id.
    let mut other = envelope(Kind::Application, epoch, private);
    other.group_id = b"another-group".to_vec();
    let rejected = w.admin_stream.rejected(other).await;
    assert_eq!(reason(&rejected), pb::RejectionReason::Malformed);
    // Garbage, and a short msg_id.
    let rejected = w
        .admin_stream
        .rejected(envelope(Kind::Application, epoch, b"garbage".to_vec()))
        .await;
    assert_eq!(reason(&rejected), pb::RejectionReason::Malformed);
    let mut short = envelope(Kind::Application, epoch, w.admin.message(b"x"));
    short.msg_id = vec![1, 2, 3];
    let rejected = w.admin_stream.rejected(short).await;
    assert_eq!(reason(&rejected), pb::RejectionReason::Malformed);
    assert_eq!(last_seq(&w.pi).await, before);
    w.laptop_stream.assert_quiet().await;
}

// spec: message-delivery/framing-matches-the-envelope
#[tokio::test]
async fn client_sent_welcome_is_malformed_and_delivers_nothing() {
    let mut w = world().await;
    let (_, carol_phone, mut carol_stream) = carol(&w).await;
    let key_packages = fetch(&w.conn, &w.admin, &TestUser::new("carol")).await;
    let epoch = w.admin.epoch();
    let (_commit, welcome) = w.admin.add(&key_packages);
    w.admin.abort();
    let mut sent = envelope(Kind::Welcome, epoch + 1, welcome);
    sent.welcome = None;
    let rejected = w.admin_stream.rejected(sent).await;
    assert_eq!(reason(&rejected), pb::RejectionReason::Malformed);
    carol_stream.assert_quiet().await;
    assert!(queued(&w.pi, &carol_phone).await.is_empty());
}

// spec: message-delivery/welcome-travels-with-its-commit
#[tokio::test]
async fn commit_and_welcome_are_accepted_together() {
    let mut w = world().await;
    let (carol, mut carol_phone, mut carol_stream) = carol(&w).await;
    let key_packages = fetch(&w.conn, &w.admin, &carol).await;
    let epoch = w.admin.epoch();
    let (commit, welcome) = w.admin.add(&key_packages);
    let sent = commit_envelope(
        epoch,
        commit.clone(),
        Some((welcome.clone(), &[&carol_phone])),
    );
    let msg_id = sent.msg_id.clone();
    let seq = w.admin_stream.accepted(sent).await;
    w.admin.confirm();

    let got = carol_stream.envelope().await;
    assert_eq!(got.kind, Kind::Welcome as i32);
    assert_eq!(got.ciphertext, welcome);
    assert_eq!(got.server_seq, seq);
    assert_eq!(got.msg_id, msg_id);
    assert_eq!(got.sender, Some(w.admin.device_ref()));
    assert_eq!(got.epoch, epoch + 1);
    assert!(got.welcome.is_none());
    carol_phone.join(&got.ciphertext);
    for (device, stream) in [
        (&mut w.laptop, &mut w.laptop_stream),
        (&mut w.phone, &mut w.phone_stream),
    ] {
        let got = stream.envelope().await;
        assert_eq!(got.kind, Kind::Commit as i32);
        assert_eq!(got.ciphertext, commit);
        assert!(got.welcome.is_none());
        device.receive_commit(&got.ciphertext);
    }
    // Carol is in: she reads the next message.
    say(&mut w.admin, &mut w.admin_stream, b"welcome Carol").await;
    let got = carol_stream.envelope().await;
    assert_eq!(carol_phone.read_message(&got.ciphertext), b"welcome Carol");
}

// spec: message-delivery/welcome-travels-with-its-commit
#[tokio::test]
async fn rejected_commit_drops_its_welcome_and_stores_nothing() {
    let mut w = world().await;
    let (carol, carol_phone, mut carol_stream) = carol(&w).await;
    let key_packages = fetch(&w.conn, &w.admin, &carol).await;
    // The laptop moves the group on first, so the admin's commit is stale.
    let epoch = w.laptop.epoch();
    let update = w.laptop.self_update();
    w.laptop_stream
        .accepted(commit_envelope(epoch, update, None))
        .await;
    w.laptop.confirm();
    let before = last_seq(&w.pi).await;

    let (commit, welcome) = w.admin.add(&key_packages);
    let rejected = w
        .admin_stream
        .rejected(commit_envelope(
            epoch,
            commit,
            Some((welcome, &[&carol_phone])),
        ))
        .await;
    assert_eq!(reason(&rejected), pb::RejectionReason::StaleEpoch);
    assert_eq!(rejected.current_epoch, epoch + 1);
    w.admin.abort();
    assert_eq!(last_seq(&w.pi).await, before);
    carol_stream.assert_quiet().await;
    assert!(queued(&w.pi, &carol_phone).await.is_empty());
}

// spec: message-delivery/welcome-travels-with-its-commit
#[tokio::test]
async fn welcome_bundle_must_name_exactly_the_added_devices() {
    let mut w = world().await;
    let (carol, carol_phone, mut carol_stream) = carol(&w).await;
    let before = last_seq(&w.pi).await;
    let epoch = w.admin.epoch();

    // Adds Carol's phone, but the bundle also lists Bob's phone.
    let key_packages = fetch(&w.conn, &w.admin, &carol).await;
    let (commit, welcome) = w.admin.add(&key_packages);
    let rejected = w
        .admin_stream
        .rejected(commit_envelope(
            epoch,
            commit.clone(),
            Some((welcome.clone(), &[&carol_phone, &w.phone])),
        ))
        .await;
    assert_eq!(reason(&rejected), pb::RejectionReason::Malformed);
    // No bundle at all for a commit that adds.
    let rejected = w
        .admin_stream
        .rejected(commit_envelope(epoch, commit.clone(), None))
        .await;
    assert_eq!(reason(&rejected), pb::RejectionReason::Malformed);
    // A bundle whose Welcome does not parse.
    let rejected = w
        .admin_stream
        .rejected(commit_envelope(
            epoch,
            commit,
            Some((b"not a welcome".to_vec(), &[&carol_phone])),
        ))
        .await;
    assert_eq!(reason(&rejected), pb::RejectionReason::Malformed);
    w.admin.abort();

    // A bundle on a commit that adds nobody, and on an application message.
    let update = w.admin.self_update();
    let rejected = w
        .admin_stream
        .rejected(commit_envelope(
            epoch,
            update,
            Some((welcome.clone(), &[&carol_phone])),
        ))
        .await;
    assert_eq!(reason(&rejected), pb::RejectionReason::Malformed);
    w.admin.abort();
    let mut app = envelope(Kind::Application, epoch, w.admin.message(b"x"));
    app.welcome = Some(pb::WelcomeBundle {
        welcome,
        recipients: vec![carol_phone.device_ref()],
    });
    let rejected = w.admin_stream.rejected(app).await;
    assert_eq!(reason(&rejected), pb::RejectionReason::Malformed);

    assert_eq!(last_seq(&w.pi).await, before);
    carol_stream.assert_quiet().await;
}

// spec: message-delivery/accepted-means-durable
// spec: pi-runtime/durable-storage
#[tokio::test]
async fn entries_are_on_disk_at_accepted_and_survive_a_restart() {
    let mut w = world().await;
    drop(w.phone_stream);
    drop(w.laptop_stream);
    let seq = say(&mut w.admin, &mut w.admin_stream, b"before the power cut").await;
    // At Accepted, the queue rows are committed.
    assert_eq!(queued(&w.pi, &w.phone).await, vec![seq]);
    assert_eq!(queued(&w.pi, &w.laptop).await, vec![seq]);

    w.pi.restart().await;
    let conn = w.pi.connect().await;
    for device in [&mut w.phone, &mut w.laptop] {
        device.authenticate(&conn).await;
        let mut stream = Stream::open(&conn, device).await;
        let got = stream.envelope().await;
        assert_eq!(got.server_seq, seq);
        assert_eq!(
            device.read_message(&got.ciphertext),
            b"before the power cut"
        );
    }
}

// spec: message-delivery/idempotent-sends
#[tokio::test]
async fn retry_after_a_lost_ack_gets_the_original_seq_and_is_delivered_once() {
    let mut w = world().await;
    let epoch = w.admin.epoch();
    let sent = envelope(Kind::Application, epoch, w.admin.message(b"once"));
    let seq = w.admin_stream.accepted(sent.clone()).await;
    let got = w.laptop_stream.envelope().await;
    assert_eq!(got.server_seq, seq);

    // The retry gets the original result, also after the epoch moved on.
    assert_eq!(w.admin_stream.accepted(sent.clone()).await, seq);
    let update = w.phone.self_update();
    let commit_seq = w
        .phone_stream
        .accepted(commit_envelope(epoch, update, None))
        .await;
    w.phone.confirm();
    assert_eq!(w.admin_stream.accepted(sent).await, seq);
    // The laptop's next entry is the commit: the message came once.
    assert_eq!(w.laptop_stream.envelope().await.server_seq, commit_seq);
    w.laptop_stream.assert_quiet().await;
}

// spec: message-delivery/idempotent-sends
#[tokio::test]
async fn reused_msg_id_with_other_content_is_duplicate_mismatch() {
    let mut w = world().await;
    let (carol, carol_phone, _carol_stream) = carol(&w).await;
    let epoch = w.admin.epoch();
    let first = envelope(Kind::Application, epoch, w.admin.message(b"one"));
    w.admin_stream.accepted(first.clone()).await;
    let mut second = envelope(Kind::Application, epoch, w.admin.message(b"two"));
    second.msg_id = first.msg_id.clone();
    let rejected = w.admin_stream.rejected(second).await;
    assert_eq!(reason(&rejected), pb::RejectionReason::DuplicateMismatch);

    // Same commit, another Welcome bundle.
    let key_packages = fetch(&w.conn, &w.admin, &carol).await;
    let (commit, welcome) = w.admin.add(&key_packages);
    let sent = commit_envelope(epoch, commit, Some((welcome, &[&carol_phone])));
    w.admin_stream.accepted(sent.clone()).await;
    w.admin.confirm();
    let mut other = sent.clone();
    if let Some(bundle) = other.welcome.as_mut() {
        bundle.welcome.push(0);
    }
    let rejected = w.admin_stream.rejected(other).await;
    assert_eq!(reason(&rejected), pb::RejectionReason::DuplicateMismatch);
}

// spec: message-delivery/idempotent-sends
#[tokio::test]
async fn rejected_envelopes_leave_no_dedup_record() {
    let mut w = world().await;
    // Rejected as stale first...
    let epoch = w.laptop.epoch();
    let update = w.laptop.self_update();
    w.laptop_stream
        .accepted(commit_envelope(epoch, update, None))
        .await;
    w.laptop.confirm();
    let stale = envelope(Kind::Application, epoch, w.admin.message(b"late"));
    let rejected = w.admin_stream.rejected(stale.clone()).await;
    assert_eq!(reason(&rejected), pb::RejectionReason::StaleEpoch);
    // ...then resent with the same msg_id in the current epoch: accepted.
    receive(&mut w.admin_stream, &mut w.admin).await;
    let mut resent = envelope(Kind::Application, w.admin.epoch(), w.admin.message(b"late"));
    resent.msg_id = stale.msg_id;
    w.admin_stream.accepted(resent).await;
}

// spec: message-delivery/strict-epoch-rule
#[tokio::test]
async fn message_sent_across_a_commit_is_stale_with_the_current_epoch() {
    let mut w = world().await;
    let epoch = w.admin.epoch();
    let update = w.admin.self_update();
    w.admin_stream
        .accepted(commit_envelope(epoch, update, None))
        .await;
    w.admin.confirm();
    let old = w.phone.message(b"old epoch");
    let rejected = w
        .phone_stream
        .rejected(envelope(Kind::Application, epoch, old))
        .await;
    assert_eq!(reason(&rejected), pb::RejectionReason::StaleEpoch);
    assert_eq!(rejected.current_epoch, epoch + 1);
    // The client processes the commit, re-encrypts and resends.
    receive(&mut w.phone_stream, &mut w.phone).await;
    say(&mut w.phone, &mut w.phone_stream, b"new epoch").await;
}

// spec: message-delivery/strict-epoch-rule
#[tokio::test]
async fn racing_commits_first_wins_second_is_stale() {
    let mut w = world().await;
    let epoch = w.laptop.epoch();
    let first = w.laptop.self_update();
    let second = w.phone.self_update();
    w.laptop_stream
        .accepted(commit_envelope(epoch, first, None))
        .await;
    w.laptop.confirm();
    let rejected = w
        .phone_stream
        .rejected(commit_envelope(epoch, second, None))
        .await;
    assert_eq!(reason(&rejected), pb::RejectionReason::StaleEpoch);
    assert_eq!(rejected.current_epoch, epoch + 1);
    w.phone.abort();
    let group_epoch: i64 =
        w.pi.query(|db| db.query_row("SELECT epoch FROM mls_groups", [], |r| r.get(0)))
            .await;
    assert_eq!(group_epoch as u64, epoch + 1);
}

// spec: message-delivery/per-device-queues
#[tokio::test]
async fn senders_other_devices_receive_the_message() {
    let mut w = world().await;
    let mut tablet = support::link_device(&w.conn, &w.alice, &w.laptop).await;
    tablet.upload_key_packages(&w.conn, 10).await;
    let mut tablet_stream = Stream::open(&w.conn, &tablet).await;
    let key_packages = conn_fetch_device(&w, &tablet).await;
    add_over_stream(
        &mut w.laptop,
        &mut w.laptop_stream,
        &key_packages,
        &mut [
            (&mut w.admin, &mut w.admin_stream),
            (&mut w.phone, &mut w.phone_stream),
        ],
        &mut [(&mut tablet, &mut tablet_stream)],
    )
    .await;

    let seq = say(&mut w.laptop, &mut w.laptop_stream, b"from my laptop").await;
    let got = tablet_stream.envelope().await;
    assert_eq!(got.server_seq, seq);
    assert_eq!(tablet.read_message(&got.ciphertext), b"from my laptop");
    assert_eq!(got.sender, Some(w.laptop.device_ref()));
}

async fn conn_fetch_device(w: &World, device: &TestDevice) -> Vec<Vec<u8>> {
    w.conn
        .keys()
        .fetch_key_packages(support::harness::authed(
            w.laptop.token(),
            pb::FetchKeyPackagesRequest {
                user_id: Some(w.alice.pb_user_id()),
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

/// Puts `n` application entries into `device`'s queue directly.
async fn fill_queue(pi: &TestPi, device: DeviceId, n: i64) {
    let at = server::runtime::clock::unix_millis(pi.clock_now());
    pi.state
        .store
        .call(move |conn| {
            let tx = conn.transaction()?;
            let last: i64 =
                tx.query_row("SELECT last_server_seq FROM sequence", [], |r| r.get(0))?;
            for seq in last + 1..=last + n {
                tx.execute(
                    "INSERT INTO envelopes (server_seq, msg_id, kind, epoch, ciphertext, accepted_at)
                     VALUES (?1, randomblob(16), 1, 1, x'00', ?2)",
                    params![seq, at],
                )?;
                tx.execute(
                    "INSERT INTO deliveries (device_id, server_seq, welcome) VALUES (?1, ?2, 0)",
                    params![device.as_slice(), seq],
                )?;
            }
            tx.execute(
                "UPDATE sequence SET last_server_seq = ?1",
                params![last + n],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
        .check("fill queue");
}

// spec: message-delivery/per-device-queues
// spec: group-membership/inactive-devices-are-kept-out
#[tokio::test]
async fn full_queue_accepts_for_everyone_else_and_marks_the_device_inactive() {
    let mut w = world().await;
    drop(w.phone_stream);
    fill_queue(&w.pi, w.phone.device_id, 1_000).await;

    let seq = say(&mut w.admin, &mut w.admin_stream, b"one too many").await;
    let got = w.laptop_stream.envelope().await;
    assert_eq!(got.server_seq, seq);
    assert_eq!(w.laptop.read_message(&got.ciphertext), b"one too many");
    // Not queued for the phone, whose queue is gone: it is inactive.
    assert!(queued(&w.pi, &w.phone).await.is_empty());
    let inactive: i64 =
        w.pi.query(|db| db.query_row("SELECT count(*) FROM inactive_devices", [], |r| r.get(0)))
            .await;
    assert_eq!(inactive, 1);

    // The Pi proposes removing the phone; any member may commit it.
    for (device, stream) in [
        (&mut w.laptop, &mut w.laptop_stream),
        (&mut w.admin, &mut w.admin_stream),
    ] {
        let proposal = receive(stream, device).await;
        assert_eq!(proposal.kind, Kind::Proposal as i32);
        assert_eq!(proposal.sender, None);
    }
    let epoch = w.laptop.epoch();
    let commit = w.laptop.commit_pending();
    w.laptop_stream
        .accepted(commit_envelope(epoch, commit, None))
        .await;
    w.laptop.confirm();
    let members =
        w.pi.state
            .store
            .call(|db| server::membership::GroupState::main_members(db))
            .await
            .check("members");
    assert!(members.iter().all(|m| m.device_id != w.phone.device_id));

    // Later messages are not queued for it either.
    receive(&mut w.admin_stream, &mut w.admin).await;
    say(&mut w.admin, &mut w.admin_stream, b"after").await;
    assert!(queued(&w.pi, &w.phone).await.is_empty());
}

// spec: message-delivery/commit-log
#[tokio::test]
async fn commits_are_kept_for_a_member_offline_for_five_days() {
    let mut w = world().await;
    drop(w.phone_stream);
    let epoch = w.admin.epoch();
    let update = w.admin.self_update();
    let seq = w
        .admin_stream
        .accepted(commit_envelope(epoch, update, None))
        .await;
    w.admin.confirm();
    w.pi.clock.advance(Duration::from_secs(5 * 24 * 3600));
    w.pi.sweep().await;
    w.phone.authenticate(&w.conn).await;
    let mut stream = Stream::open(&w.conn, &w.phone).await;
    let got = receive(&mut stream, &mut w.phone).await;
    assert_eq!(got.kind, Kind::Commit as i32);
    assert_eq!(got.server_seq, seq);
    assert_eq!(w.phone.epoch(), w.admin.epoch());

    // Acknowledged by every device that must receive it, it is deleted.
    receive(&mut w.laptop_stream, &mut w.laptop).await;
    w.laptop_stream.sync().await;
    stream.sync().await;
    let kept: i64 =
        w.pi.query(move |db| {
            db.query_row(
                "SELECT count(*) FROM envelopes WHERE server_seq = ?1",
                params![seq as i64],
                |r| r.get(0),
            )
        })
        .await;
    assert_eq!(kept, 0);
}

// spec: message-delivery/commit-log
// spec: message-delivery/expiry-and-gap-notice
#[tokio::test]
async fn connected_device_that_never_acks_loses_messages_but_not_commits() {
    let mut w = world().await;
    let m1 = say(&mut w.admin, &mut w.admin_stream, b"m1").await;
    let m1_at = w.pi.clock_now();
    let epoch = w.admin.epoch();
    let update = w.admin.self_update();
    let commit = w
        .admin_stream
        .accepted(commit_envelope(epoch, update, None))
        .await;
    w.admin.confirm();
    // The laptop receives both but never acks.
    assert_eq!(w.laptop_stream.envelope().await.server_seq, m1);
    assert_eq!(w.laptop_stream.envelope().await.server_seq, commit);

    w.pi.clock.advance(Duration::from_secs(7 * 24 * 3600 + 1));
    w.pi.sweep().await;
    match w.laptop_stream.next().await {
        Out::GapNotice(gap) => {
            assert_eq!(gap.group_id, GROUP_ID);
            assert_eq!(
                gap.newest_expired_at,
                Some(prost_types::Timestamp::from(m1_at))
            );
        }
        other => panic!("expected a gap notice, got {other:?}"),
    }
    assert_eq!(queued(&w.pi, &w.laptop).await, vec![commit]);

    // On reconnect only the commit comes again.
    w.laptop.authenticate(&w.conn).await;
    let mut stream = Stream::open(&w.conn, &w.laptop).await;
    assert_eq!(stream.envelope().await.server_seq, commit);
    stream.assert_quiet().await;
}

// spec: message-delivery/ordered-delivery-and-catch-up
#[tokio::test]
async fn catch_up_across_an_epoch_change_then_live_traffic() {
    let mut w = world().await;
    drop(w.phone_stream);
    let m1 = say(&mut w.admin, &mut w.admin_stream, b"M1").await;
    let epoch = w.admin.epoch();
    let update = w.admin.self_update();
    let c = w
        .admin_stream
        .accepted(commit_envelope(epoch, update, None))
        .await;
    w.admin.confirm();
    let m2 = say(&mut w.admin, &mut w.admin_stream, b"M2").await;

    let mut stream = Stream::open(&w.conn, &w.phone).await;
    let got = stream.envelope().await;
    assert_eq!(got.server_seq, m1);
    assert_eq!(w.phone.read_message(&got.ciphertext), b"M1");
    let got = stream.envelope().await;
    assert_eq!(got.server_seq, c);
    w.phone.receive_commit(&got.ciphertext);
    let got = stream.envelope().await;
    assert_eq!(got.server_seq, m2);
    assert_eq!(w.phone.read_message(&got.ciphertext), b"M2");

    for _ in 0..3 {
        receive(&mut w.laptop_stream, &mut w.laptop).await;
    }
    let m3 = say(&mut w.laptop, &mut w.laptop_stream, b"live").await;
    let got = stream.envelope().await;
    assert_eq!(got.server_seq, m3);
    assert!(m1 < c && c < m2 && m2 < m3);
}

// spec: message-delivery/ordered-delivery-and-catch-up
#[tokio::test]
async fn committer_reads_messages_before_its_commit_before_accepted() {
    let mut w = world().await;
    // Bob's message M and Alice's commit C go out at the same moment, round
    // after round. When M is sequenced first, Alice's stream task is busy
    // with C while M's wake waits, so without the ordering `Accepted` for C
    // would come before M.
    let mut m_first = 0;
    for round in 0..20 {
        let epoch = w.laptop.epoch();
        let m = envelope(Kind::Application, epoch, w.phone.message(b"M"));
        let m_id = m.msg_id.clone();
        let c = commit_envelope(epoch, w.laptop.self_update(), None);
        let c_id = c.msg_id.clone();
        w.phone_stream.send_envelope(m).await;
        w.laptop_stream.send_envelope(c).await;

        let bob = w.phone_stream.result().await;
        assert_eq!(bob.msg_id, m_id);
        let m_seq = match bob.result {
            Some(pb::send_result::Result::Accepted(accepted)) => Some(accepted.server_seq),
            Some(pb::send_result::Result::Rejected(rejected)) => {
                assert_eq!(reason(&rejected), pb::RejectionReason::StaleEpoch);
                None
            }
            None => panic!("round {round}: empty result"),
        };
        if let Some(m_seq) = m_seq {
            // Alice's stream: M first, read in the old epoch...
            let got = w.laptop_stream.envelope().await;
            assert_eq!(got.server_seq, m_seq, "round {round}");
            assert_eq!(w.laptop.read_message(&got.ciphertext), b"M");
            m_first += 1;
        }
        // ...then `Accepted` for C, and only now does she merge it.
        let result = match w.laptop_stream.next().await {
            Out::SendResult(result) => result,
            other => panic!("round {round}: expected Accepted for C, got {other:?}"),
        };
        assert_eq!(result.msg_id, c_id);
        let Some(pb::send_result::Result::Accepted(accepted)) = result.result else {
            panic!("round {round}: expected Accepted, got {result:?}");
        };
        assert!(m_seq.is_none_or(|m| m < accepted.server_seq));
        w.laptop.confirm();
        w.laptop_stream.ack(accepted.server_seq).await;
        receive(&mut w.phone_stream, &mut w.phone).await;
        if m_seq.is_some() {
            receive(&mut w.admin_stream, &mut w.admin).await;
        }
        receive(&mut w.admin_stream, &mut w.admin).await;
    }
    assert!(m_first > 0, "M was never sequenced before C");
}

// spec: message-delivery/acknowledgement
#[tokio::test]
async fn ack_deletes_entries_up_to_it_and_reconnect_starts_after() {
    let mut w = world().await;
    let s1 = say(&mut w.admin, &mut w.admin_stream, b"1").await;
    let s2 = say(&mut w.admin, &mut w.admin_stream, b"2").await;
    let s3 = say(&mut w.admin, &mut w.admin_stream, b"3").await;
    for expected in [s1, s2, s3] {
        assert_eq!(w.phone_stream.envelope().await.server_seq, expected);
    }
    w.phone_stream.ack(s2).await;
    w.phone_stream.sync().await;
    assert_eq!(queued(&w.pi, &w.phone).await, vec![s3]);

    let mut stream = Stream::open(&w.conn, &w.phone).await;
    assert_eq!(stream.envelope().await.server_seq, s3);
    stream.assert_quiet().await;
}

// spec: message-delivery/expiry-and-gap-notice
#[tokio::test]
async fn device_connecting_after_its_entries_expired_gets_one_gap_notice() {
    let mut w = world().await;
    let m1 = say(&mut w.admin, &mut w.admin_stream, b"m1").await;
    let m1_at = w.pi.clock_now();
    // The phone receives it but never acks, and leaves after six days.
    assert_eq!(w.phone_stream.envelope().await.server_seq, m1);
    w.pi.clock.advance(Duration::from_secs(6 * 24 * 3600));
    // Its stream idles out now; it was connected until then.
    support::messaging::wait_offline(&w.pi, &w.phone).await;
    w.pi.clock.advance(Duration::from_secs(24 * 3600 + 1));
    w.pi.sweep().await;
    assert!(queued(&w.pi, &w.phone).await.is_empty());

    // Back after a day: not inactive, but told what expired.
    let mut stream = support::messaging::reconnect(&w.conn, &mut w.phone).await;
    match stream.next().await {
        Out::GapNotice(gap) => {
            assert_eq!(
                gap.newest_expired_at,
                Some(prost_types::Timestamp::from(m1_at))
            );
        }
        other => panic!("expected a gap notice, got {other:?}"),
    }
    stream.assert_quiet().await;
    // Only once.
    let mut again = support::messaging::reconnect(&w.conn, &mut w.phone).await;
    again.assert_quiet().await;
}

// spec: group-membership/removing-a-user
#[tokio::test]
async fn revoked_device_receives_no_further_envelopes() {
    let mut w = world().await;
    w.conn
        .admin()
        .remove_user(support::harness::authed(
            w.admin.token(),
            pb::RemoveUserRequest {
                user_id: Some(w.bob.pb_user_id()),
            },
        ))
        .await
        .check("RemoveUser");
    // Even before the admin commits the Removes, nothing is queued for Bob.
    say(&mut w.admin, &mut w.admin_stream, b"after Bob left").await;
    w.laptop_stream.envelope().await;
    assert!(queued(&w.pi, &w.phone).await.is_empty());
}
