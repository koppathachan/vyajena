//! group-membership: Automatic add of new users, the core's side, end to end
//! against the real server (flutter-app task 4.5): the admin's device adds a new
//! user's device on the server's `JoinNotice` with no `add_user` call, pins the
//! user's identity key on first sight, waits for the admin to accept a
//! changed key, and a losing admin device does nothing more.

mod support;

use client::{Event, SendStatus};
use support::Check;
use support::device::{self, Device, member, user};
use support::server::{ADMIN, TestServer};

fn joined(e: &Event) -> bool {
    matches!(e, Event::JoinedGroup)
}

// spec: group-membership/automatic-add-of-new-users
#[tokio::test(flavor = "multi_thread")]
async fn bob_joins_while_the_admin_is_online() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    // Nobody calls add_user: Bob signs up and the admin's device adds him.
    let mut bob = device::invited(&admin, "bob").await;
    bob.wait_event("bob joined", joined).await;
    admin.wait_members(&[member(&admin), member(&bob)]).await;
    bob.wait_members(&[member(&admin), member(&bob)]).await;

    // Bob's identity key was pinned on the admin's device by the add: no new
    // key is pending.
    assert!(
        !admin
            .client
            .accept_identity_key(&user("bob"))
            .check("accept")
    );
    // And they chat.
    let id = bob.client.send_text("hi").check("send");
    bob.wait_event(
        "hi accepted",
        |e| matches!(e, Event::SendStatus { id: i, status: SendStatus::Accepted } if *i == id),
    )
    .await;
    admin
        .wait_event("hi arrives", |e| matches!(e, Event::Message(_)))
        .await;
}

// spec: group-membership/automatic-add-of-new-users
#[tokio::test(flavor = "multi_thread")]
async fn admin_offline_when_bob_joins_adds_him_on_connect() {
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let mut bob = device::invited_while_admin_away(&admin, "bob").await;
    // Waiting to be added: no Welcome while no admin device is online.
    assert!(!bob.sees_within_quiet(joined).await);
    assert_eq!(bob.epoch(), None);

    admin.client.start().await.check("start admin");
    bob.wait_event("bob joined", joined).await;
    admin.wait_members(&[member(&admin), member(&bob)]).await;
}

// spec: group-membership/automatic-add-of-new-users
// spec: key-directory/clients-verify-the-identity-chain
#[tokio::test(flavor = "multi_thread")]
async fn a_reset_user_is_added_once_the_admin_accepts_the_key() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let mut bob = device::invited(&admin, "bob").await;
    bob.wait_event("bob joined", joined).await;

    // The admin resets Bob; his new phone has a new identity key.
    admin
        .client
        .remove_user(&user("bob"))
        .await
        .check("remove bob");
    drop(bob);
    let mut new_bob = device::invited(&admin, "bob").await;

    // The admin's phone warns and does not add him.
    admin
        .wait_event(
            "the key-changed warning",
            |e| matches!(e, Event::IdentityKeyChanged { user: u } if *u == user("bob")),
        )
        .await;
    assert!(!new_bob.sees_within_quiet(joined).await);

    // The admin accepts the key: the phone adds him with no further tap.
    assert!(
        admin
            .client
            .accept_identity_key(&user("bob"))
            .check("accept")
    );
    new_bob.wait_event("new bob joined", joined).await;
    admin
        .wait_members(&[member(&admin), member(&new_bob)])
        .await;
}

// spec: group-membership/automatic-add-of-new-users
#[tokio::test(flavor = "multi_thread")]
async fn two_admin_devices_race_and_the_loser_does_nothing() {
    let server = TestServer::start().await;
    let mut phone = device::admin_with_group(&server).await;
    // The admin's tablet, linked and in the group.
    let mut tablet = Device::new().await;
    let request = tablet.client.link_request().await.check("link request");
    let approval = phone.client.approve_link(&request).await.check("approve");
    let id = tablet
        .client
        .complete_link(&approval)
        .await
        .check("complete");
    assert_eq!(id, user(ADMIN));
    tablet.client.start().await.check("start tablet");
    tablet.wait_event("tablet joined", joined).await;
    phone.wait_members(&[member(&phone), member(&tablet)]).await;

    // Both admin devices hear about Bob's phone and try to add it.
    let mut bob = device::invited(&phone, "bob").await;
    bob.wait_event("bob joined", joined).await;
    let all = [member(&phone), member(&tablet), member(&bob)];
    phone.wait_members(&all).await;
    tablet.wait_members(&all).await;
    // Exactly one add: one epoch for Bob's add, and one Welcome.
    let epoch = bob.epoch().check("bob's epoch");
    assert_eq!(phone.epoch(), Some(epoch));
    assert_eq!(tablet.epoch(), Some(epoch));
    assert!(!bob.sees_within_quiet(joined).await);
    assert!(
        !phone
            .sees_within_quiet(|e| matches!(e, Event::CommitRefused { .. }))
            .await
    );
    assert!(
        !tablet
            .sees_within_quiet(|e| matches!(e, Event::CommitRefused { .. }))
            .await
    );
    assert_eq!(phone.epoch(), Some(epoch));

    // Everyone still follows the group.
    let id = bob.client.send_text("hello both").check("send");
    bob.wait_event(
        "accepted",
        |e| matches!(e, Event::SendStatus { id: i, status: SendStatus::Accepted } if *i == id),
    )
    .await;
    phone
        .wait_event("arrives on the phone", |e| matches!(e, Event::Message(_)))
        .await;
    tablet
        .wait_event("arrives on the tablet", |e| matches!(e, Event::Message(_)))
        .await;
}
