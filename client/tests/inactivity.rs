//! group-membership: Inactive devices are kept out; Automatic re-add, on the
//! device, end to end against the real server (client-core task 5.3), with the
//! two owner decisions of task 6.4: a device re-added from a Welcome that
//! names a reset user re-pins that user's key (key-directory: Clients verify
//! the identity chain), and a message typed while removed goes out after the
//! re-add (client-runtime: Send status).

mod support;

use std::time::Duration;

use client::store::Store;
use client::{ConnectionState, Event, MessageBody, SendStatus};
use support::Check;
use support::device::{self, Device, member, user};
use support::server::TestServer;

const TEN_DAYS: Duration = Duration::from_secs(10 * 24 * 60 * 60);

/// Stops `device` and waits until the server has seen its stream close, so its
/// last-seen time is now and not after a clock jump.
async fn go_offline(server: &TestServer, device: &Device) {
    device.client.stop();
    server.wait_offline_recorded(&device.device_id()).await;
}

/// The one-time KeyPackages the server holds for `device`.
async fn one_time(server: &TestServer, device: &Device) -> i64 {
    let id = device.device_id();
    server
        .query(move |db| {
            db.query_row(
                "SELECT count(*) FROM key_packages WHERE device_id = ?1 AND last_resort = 0",
                [id.as_slice()],
                |row| row.get(0),
            )
        })
        .await
}

/// Moves the server's clock and waits until `online` reconnected (the server's idle
/// timeout closes streams when its clock jumps).
async fn advance_server(server: &TestServer, by: Duration, online: &mut [&mut Device]) {
    server.clock.advance(by);
    for device in online.iter_mut() {
        device
            .wait_event("the stream to be closed", |e| {
                matches!(e, Event::Connection(ConnectionState::Connecting))
            })
            .await;
        device.wait_connected().await;
    }
}

// spec: group-membership/inactive-devices-are-kept-out
// spec: group-membership/automatic-re-add
// spec: client-runtime/events-in-order
#[tokio::test(flavor = "multi_thread")]
async fn back_from_holiday_the_admin_re_adds_the_phone_with_no_one_tapping() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    bob.wait_epoch(1).await;
    go_offline(&server, &bob).await;

    advance_server(&server, TEN_DAYS, &mut [&mut admin]).await;
    server.sweep().await;
    // The admin commits the server's inactivity Remove.
    admin.wait_members(&[member(&admin)]).await;
    admin.wait_epoch(2).await;

    // Bob's phone is back: it hears it was removed, then gets re-added.
    bob.client.start().await.check("start bob");
    let removed = bob
        .wait_event("removed for inactivity", |e| {
            matches!(e, Event::RemovedForInactivity { .. })
        })
        .await;
    let Event::RemovedForInactivity { missed_since } = removed else {
        unreachable!()
    };
    assert!(missed_since <= server.now());
    bob.wait_event("rejoined", |e| matches!(e, Event::Rejoined))
        .await;
    let both = [member(&admin), member(&bob)];
    admin.wait_members(&both).await;
    bob.wait_members(&both).await;
    admin.wait_epoch(3).await;
    bob.wait_epoch(3).await;
}

// spec: group-membership/automatic-re-add
// spec: device-identity/device-linking
// spec: key-directory/keypackage-fetch
#[tokio::test(flavor = "multi_thread")]
async fn two_devices_race_to_re_add_and_the_loser_does_nothing_more() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let mut phone = device::member_of_group(&admin, "bob").await;
    // Bob links a tablet; his phone adds it.
    let mut tablet = Device::new().await;
    let request = tablet.client.link_request().await.check("link request");
    let approval = phone.client.approve_link(&request).await.check("approve");
    tablet
        .client
        .complete_link(&approval)
        .await
        .check("complete");
    tablet.client.start().await.check("start tablet");
    tablet
        .wait_event("the tablet joined", |e| matches!(e, Event::JoinedGroup))
        .await;
    let all = [member(&admin), member(&phone), member(&tablet)];
    admin.wait_members(&all).await;
    admin.wait_epoch(2).await;
    go_offline(&server, &phone).await;

    advance_server(&server, TEN_DAYS, &mut [&mut admin, &mut tablet]).await;
    server.sweep().await;
    let without_phone = [member(&admin), member(&tablet)];
    admin.wait_members(&without_phone).await;
    tablet.wait_members(&without_phone).await;

    // Both the tablet and the admin hear the phone is back and race. Each
    // fetches a KeyPackage for the phone only.
    let tablet_before = one_time(&server, &tablet).await;
    let admin_before = one_time(&server, &admin).await;
    phone.client.start().await.check("start phone");
    phone
        .wait_event("rejoined", |e| matches!(e, Event::Rejoined))
        .await;
    for device in [&admin, &tablet, &phone] {
        device.wait_members(&all).await;
    }
    let epoch = admin.epoch().check("admin epoch");
    tablet.wait_epoch(epoch).await;
    phone.wait_epoch(epoch).await;
    // One Welcome only, and nobody refused anything.
    assert!(
        !phone
            .sees_within_quiet(|e| matches!(e, Event::Rejoined))
            .await
    );
    assert_eq!(admin.epoch(), Some(epoch), "the loser committed again");
    assert_eq!(one_time(&server, &tablet).await, tablet_before);
    assert_eq!(one_time(&server, &admin).await, admin_before);
    for device in [&mut admin, &mut tablet, &mut phone] {
        assert!(
            !device
                .events()
                .iter()
                .any(|e| matches!(e, Event::CommitRefused { .. })),
            "{:?}",
            device.events()
        );
    }
}

// spec: group-membership/automatic-re-add
// spec: group-membership/inactive-devices-are-kept-out
#[tokio::test(flavor = "multi_thread")]
async fn nobody_able_to_re_add_is_online_so_the_phone_waits() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let mut alice = device::member_of_group(&admin, "alice").await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    alice.wait_epoch(2).await;
    go_offline(&server, &bob).await;

    advance_server(&server, TEN_DAYS, &mut [&mut admin, &mut alice]).await;
    // The admin is offline when the server notices Bob is inactive...
    go_offline(&server, &admin).await;
    server.sweep().await;
    // ... and commits the server's Remove when it next connects.
    admin.client.start().await.check("start admin");
    admin.wait_members(&[member(&admin), member(&alice)]).await;
    alice.wait_members(&[member(&admin), member(&alice)]).await;

    // Bob comes back while the admin is away again: Alice cannot re-add
    // another user's device, so he waits.
    go_offline(&server, &admin).await;
    bob.client.start().await.check("start bob");
    bob.wait_event("removed for inactivity", |e| {
        matches!(e, Event::RemovedForInactivity { .. })
    })
    .await;
    assert!(
        !bob.sees_within_quiet(|e| matches!(e, Event::Rejoined))
            .await
    );

    // The admin connects and re-adds him.
    admin.client.start().await.check("start admin");
    bob.wait_event("rejoined", |e| matches!(e, Event::Rejoined))
        .await;
    let all = [member(&admin), member(&alice), member(&bob)];
    for device in [&admin, &alice, &bob] {
        device.wait_members(&all).await;
    }
}

// spec: key-directory/clients-verify-the-identity-chain
// spec: group-membership/automatic-re-add
#[tokio::test(flavor = "multi_thread")]
async fn back_from_holiday_after_a_reset_the_laptop_joins_and_re_pins() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let mut alice = device::member_of_group(&admin, "alice").await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    alice.wait_epoch(2).await;
    go_offline(&server, &alice).await;

    // Ten days away: the admin commits the server's Remove of Alice's laptop.
    advance_server(&server, TEN_DAYS, &mut [&mut admin, &mut bob]).await;
    server.sweep().await;
    admin.wait_members(&[member(&admin), member(&bob)]).await;

    // Meanwhile the admin resets Bob: his new phone has a new identity key.
    admin
        .client
        .remove_user(&user("bob"))
        .await
        .check("remove bob");
    drop(bob);
    let mut new_bob = device::invited(&admin, "bob").await;
    let refused = admin.client.add_user(&user("bob")).await;
    assert!(
        matches!(&refused, Err(client::ClientError::IdentityKeyChanged(_))),
        "{refused:?}"
    );
    assert!(
        admin
            .client
            .accept_identity_key(&user("bob"))
            .check("accept")
    );
    admin
        .client
        .add_user(&user("bob"))
        .await
        .check("add new bob");
    new_bob
        .wait_event("joined", |e| matches!(e, Event::JoinedGroup))
        .await;

    // Alice's laptop is back with Bob's old key pinned. The admin re-adds
    // it; its Welcome holds Bob's new phone. The laptop looks Bob's key up,
    // joins, re-pins and warns.
    alice.client.start().await.check("start alice");
    alice
        .wait_event("rejoined", |e| matches!(e, Event::Rejoined))
        .await;
    alice
        .wait_event(
            "the key-changed warning",
            |e| matches!(e, Event::IdentityKeyChanged { user: u } if *u == user("bob")),
        )
        .await;
    let everyone = [member(&admin), member(&alice), member(&new_bob)];
    alice.wait_members(&everyone).await;
    assert!(
        !alice
            .client
            .accept_identity_key(&user("bob"))
            .check("nothing pending"),
        "the new key is already pinned"
    );
    assert!(
        !alice
            .events()
            .iter()
            .any(|e| matches!(e, Event::CommitRefused { .. })),
        "{:?}",
        alice.events()
    );

    // The group goes on: Alice reads what new Bob writes.
    let id = new_bob.client.send_text("hi alice").check("send");
    new_bob
        .wait_event(
            "accepted",
            |e| matches!(e, Event::SendStatus { id: i, status: SendStatus::Accepted } if *i == id),
        )
        .await;
    alice
        .wait_event(
            "the message",
            |e| matches!(e, Event::Message(m) if m.body == MessageBody::Text("hi alice".into())),
        )
        .await;
}

// spec: client-runtime/send-status
// spec: group-membership/automatic-re-add
// spec: group-membership/orphaned-device-self-recovery
#[tokio::test(flavor = "multi_thread")]
async fn a_message_typed_while_removed_goes_out_after_the_re_add() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    bob.wait_epoch(1).await;
    go_offline(&server, &bob).await;

    advance_server(&server, TEN_DAYS, &mut [&mut admin]).await;
    server.sweep().await;
    admin.wait_members(&[member(&admin)]).await;
    // Nobody who can re-add Bob is online when he comes back.
    go_offline(&server, &admin).await;

    bob.client.start().await.check("start bob");
    bob.wait_event("removed for inactivity", |e| {
        matches!(e, Event::RemovedForInactivity { .. })
    })
    .await;
    let id = bob.client.send_text("back").check("send while removed");
    let msg_id = Store::open(&bob.data_dir().join(client::DATABASE_FILE), &bob.keys)
        .check("open bob's store")
        .message(id)
        .check("read")
        .check("the message")
        .msg_id;
    // group-resync G1/G2: holding a stale group yet caught up, Bob surfaces the
    // chat as out of sync, the held send as out of sync (not a silent hourglass
    // and not failed), and self-requests re-add even though it holds group
    // state.
    bob.wait_event(
        "the chat is out of sync",
        |e| matches!(e, Event::OutOfSync { chat } if *chat == client::ChatId::MAIN),
    )
    .await;
    bob.wait_event(
        "the held send is out of sync",
        |e| matches!(e, Event::SendStatus { id: i, status: SendStatus::OutOfSync } if *i == id),
    )
    .await;
    bob.wait_event("asked to be re-added", |e| {
        matches!(e, Event::ReaddRequested { .. })
    })
    .await;
    assert!(
        !bob.events().iter().any(|e| matches!(
            e,
            Event::SendStatus { id: i, status: SendStatus::Failed { .. } } if *i == id
        )),
        "the held message must not be reported failed: {:?}",
        bob.events()
    );

    // The admin connects and re-adds Bob; "back" goes out in the new epoch
    // with the same msg_id and is accepted.
    admin.client.start().await.check("start admin");
    bob.wait_event("rejoined", |e| matches!(e, Event::Rejoined))
        .await;
    bob.wait_event(
        "accepted",
        |e| matches!(e, Event::SendStatus { id: i, status: SendStatus::Accepted } if *i == id),
    )
    .await;
    admin
        .wait_event(
            "the message",
            |e| matches!(e, Event::Message(m) if m.body == MessageBody::Text("back".into())),
        )
        .await;
    // The server accepted exactly one envelope with that msg_id from Bob's
    // laptop: the one Bob's history shows as accepted. The admin, who has
    // only the new epoch's keys, read it.
    let accepted_at = bob
        .client
        .message(id)
        .check("message")
        .check("the message")
        .accepted_at;
    assert!(accepted_at.is_some());
    let bob_device = bob.device_id();
    let accepted: i64 = server
        .query(move |db| {
            db.query_row(
                "SELECT count(*) FROM dedup WHERE sender_device_id = ?1 AND msg_id = ?2",
                [bob_device.as_slice(), msg_id.as_slice()],
                |row| row.get(0),
            )
        })
        .await;
    assert_eq!(accepted, 1);
    assert!(
        !bob.events().iter().any(|e| matches!(
            e,
            Event::SendStatus {
                status: SendStatus::Failed { .. },
                ..
            }
        )),
        "{:?}",
        bob.events()
    );
}

// spec: client-runtime/events-in-order
// spec: group-membership/automatic-re-add
#[tokio::test(flavor = "multi_thread")]
async fn inactivity_removes_and_re_adds_are_marked_automatic() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let mut alice = device::member_of_group(&admin, "alice").await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    let everyone = [member(&admin), member(&alice), member(&bob)];
    for device in [&admin, &alice, &bob] {
        device.wait_members(&everyone).await;
    }
    let bob_member = member(&bob);
    go_offline(&server, &bob).await;
    // Skip the events so far.
    let _ = admin.sees_within_quiet(|_| false).await;
    let seen = (admin.events().len(), alice.events().len());

    advance_server(&server, TEN_DAYS, &mut [&mut admin, &mut alice]).await;
    server.sweep().await;
    let without_bob = [member(&admin), member(&alice)];
    admin.wait_members(&without_bob).await;
    alice.wait_members(&without_bob).await;

    bob.client.start().await.check("start bob");
    bob.wait_event("rejoined", |e| matches!(e, Event::Rejoined))
        .await;
    admin.wait_members(&everyone).await;
    alice.wait_members(&everyone).await;

    // Admin (own commits) and Alice (the admin's commits) both see the
    // Remove and the re-add marked automatic, so the UI stays quiet.
    for (device, from) in [(&admin, seen.0), (&alice, seen.1)] {
        let changes: Vec<Event> = device.events()[from..]
            .iter()
            .filter(|e| matches!(e, Event::MembersChanged { .. }))
            .cloned()
            .collect();
        assert!(!changes.is_empty());
        for change in &changes {
            let Event::MembersChanged {
                added,
                removed,
                automatic,
                ..
            } = change
            else {
                unreachable!()
            };
            for changed in added.iter().chain(removed) {
                assert!(automatic.contains(changed), "{changes:?}");
            }
        }
        assert!(
            changes.iter().any(|e| matches!(e,
            Event::MembersChanged { removed, .. } if removed == std::slice::from_ref(&bob_member)))
        );
        assert!(changes.iter().any(|e| matches!(e,
            Event::MembersChanged { added, .. } if added == std::slice::from_ref(&bob_member))));
    }
}
