//! message-deletion on the server (change `delete-messages`): the server
//! authorises a delete from the stream metadata (`Envelope.deletes`) and drops
//! the target's still-queued copies, pairing the retract with a delivered
//! Delete (message-delivery: Deletion retracts undelivered copies;
//! message-deletion: Who may delete; Tombstone, not erasure).
//!
//! Cast (support::messaging::World): the admin (`akhil`), Alice's laptop and
//! Bob's phone, all in the main group.

mod support;

use rusqlite::params;
use support::clients::TestDevice;
use support::harness::{TestServer, authed};
use support::messaging::{
    GROUP_ID, World, commit_envelope_in, envelope, envelope_in, offline, reconnect, wait_offline,
    world,
};
use support::pb::envelope::Kind;
use support::{Check, pb, server_external_sender};

const DM_ID: &[u8] = b"alice-and-bob-dm";

fn reason(rejected: &pb::send_result::Rejected) -> pb::RejectionReason {
    pb::RejectionReason::try_from(rejected.reason).check("reason")
}

/// Deliveries waiting for `device`.
async fn queued(server: &TestServer, device: &TestDevice) -> Vec<u64> {
    let id = device.device_id.to_vec();
    server
        .query(move |db| {
            let mut stmt = db.prepare(
                "SELECT server_seq FROM deliveries WHERE device_id = ?1 ORDER BY server_seq",
            )?;
            let rows = stmt.query_map(params![id], |r| r.get::<_, i64>(0))?;
            rows.map(|r| r.map(|n| n as u64)).collect()
        })
        .await
}

/// An `APPLICATION` envelope that deletes `(target_sender, target_msg_id)` in
/// `group_id`, carrying opaque `ciphertext` (the server never reads it).
fn delete_envelope(
    group_id: &[u8],
    epoch: u64,
    ciphertext: Vec<u8>,
    target_sender: pb::DeviceRef,
    target_msg_id: Vec<u8>,
) -> pb::Envelope {
    let mut envelope = envelope_in(group_id, Kind::Application, epoch, ciphertext);
    envelope.deletes = Some(pb::DeleteTarget {
        target_sender: Some(target_sender),
        target_msg_id,
    });
    envelope
}

// spec: message-deletion/who-may-delete
#[tokio::test]
async fn an_admin_deletes_any_message_in_the_group_but_a_member_may_not() {
    let mut w = world().await;

    // The admin deletes Bob's message: accepted.
    let epoch = w.phone.epoch();
    let bobs = envelope(Kind::Application, epoch, w.phone.message(b"from bob"));
    let bobs_msg_id = bobs.msg_id.clone();
    w.phone_stream.accepted(bobs).await;
    let epoch = w.admin.epoch();
    let carrier = w.admin.message(b"<delete>");
    let delete = delete_envelope(GROUP_ID, epoch, carrier, w.phone.device_ref(), bobs_msg_id);
    w.admin_stream.accepted(delete).await;

    // A regular member cannot delete even its own message in the group.
    let epoch = w.laptop.epoch();
    let mine = envelope(Kind::Application, epoch, w.laptop.message(b"from alice"));
    let mine_msg_id = mine.msg_id.clone();
    w.laptop_stream.accepted(mine).await;
    let epoch = w.laptop.epoch();
    let carrier = w.laptop.message(b"<delete>");
    let delete = delete_envelope(GROUP_ID, epoch, carrier, w.laptop.device_ref(), mine_msg_id);
    let rejected = w.laptop_stream.rejected(delete).await;
    assert_eq!(reason(&rejected), pb::RejectionReason::PolicyViolation);
}

// spec: message-delivery/deletion-retracts-undelivered-copies
// spec: message-deletion/tombstone-not-erasure
#[tokio::test]
async fn a_delete_retracts_undelivered_copies_and_is_still_delivered() {
    let mut w = world().await;

    // Bob's phone goes offline before the message is sent, so its copy is
    // queued and never delivered.
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;

    // The admin sends a message; it is queued for both members, unacked.
    let epoch = w.admin.epoch();
    let doomed = envelope(Kind::Application, epoch, w.admin.message(b"doomed"));
    let target_msg_id = doomed.msg_id.clone();
    let target_seq = w.admin_stream.accepted(doomed).await;
    // The online laptop receives it; the offline phone has it queued.
    assert_eq!(w.laptop_stream.envelope().await.server_seq, target_seq);
    assert!(queued(&w.server, &w.phone).await.contains(&target_seq));

    // The admin deletes it.
    let epoch = w.admin.epoch();
    let carrier = w.admin.message(b"<delete>");
    let delete = delete_envelope(
        GROUP_ID,
        epoch,
        carrier,
        w.admin.device_ref(),
        target_msg_id,
    );
    let delete_seq = w.admin_stream.accepted(delete).await;

    // The online laptop receives the delete over its stored copy
    // (tombstone-not-erasure); the server does not erase it from the laptop.
    assert_eq!(w.laptop_stream.envelope().await.server_seq, delete_seq);

    // Bob's phone never gets the original plaintext: its queued copy is gone,
    // but the delete is queued for it, so nothing is dropped silently.
    let phone_queued = queued(&w.server, &w.phone).await;
    assert!(!phone_queued.contains(&target_seq));
    assert!(phone_queued.contains(&delete_seq));

    // On reconnect the phone receives only the delete, never the original.
    let mut phone = reconnect(&w.conn, &mut w.phone).await;
    let got = phone.envelope().await;
    assert_eq!(got.server_seq, delete_seq);
    assert_eq!(got.kind, Kind::Application as i32);
    phone.ack(got.server_seq).await;
    phone.assert_quiet().await;
}

// spec: message-delivery/deletion-retracts-undelivered-copies
#[tokio::test]
async fn a_delete_of_an_unknown_target_is_accepted_and_delivered() {
    let mut w = world().await;
    // No message with this msg_id exists; the delete is still accepted.
    let epoch = w.admin.epoch();
    let carrier = w.admin.message(b"<delete>");
    let delete = delete_envelope(
        GROUP_ID,
        epoch,
        carrier,
        w.admin.device_ref(),
        support::messaging::msg_id(),
    );
    let seq = w.admin_stream.accepted(delete).await;
    assert_eq!(w.laptop_stream.envelope().await.server_seq, seq);
}

/// Alice's laptop opens the Alice–Bob DM and adds Bob's phone.
async fn dm_world() -> World {
    let mut w = world().await;
    w.laptop.upload_dm_key_packages(&w.conn, 10).await;
    w.phone.upload_dm_key_packages(&w.conn, 10).await;
    let server = server_external_sender(&w.conn).await;
    let request = w.laptop.create_dm(
        DM_ID,
        &server,
        ("alice@chat", "bob@chat"),
        w.bob.pb_user_id(),
    );
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
    w.laptop_stream.sync().await;
    w
}

// spec: message-deletion/who-may-delete
#[tokio::test]
async fn in_a_dm_only_the_senders_own_device_may_delete() {
    let mut w = dm_world().await;

    // Alice sends a DM message.
    let epoch = w.laptop.epoch();
    let alices = envelope_in(DM_ID, Kind::Application, epoch, w.laptop.message(b"hi bob"));
    let alices_msg_id = alices.msg_id.clone();
    w.laptop_stream.accepted(alices).await;
    let _ = w.phone_stream.envelope().await;

    // Bob cannot delete Alice's message: he is not its original sender.
    let epoch = w.phone.epoch();
    let carrier = w.phone.message(b"<delete>");
    let delete = delete_envelope(
        DM_ID,
        epoch,
        carrier,
        w.laptop.device_ref(),
        alices_msg_id.clone(),
    );
    let rejected = w.phone_stream.rejected(delete).await;
    assert_eq!(reason(&rejected), pb::RejectionReason::PolicyViolation);

    // Alice deletes her own message: accepted.
    let epoch = w.laptop.epoch();
    let carrier = w.laptop.message(b"<delete>");
    let delete = delete_envelope(DM_ID, epoch, carrier, w.laptop.device_ref(), alices_msg_id);
    w.laptop_stream.accepted(delete).await;
}
