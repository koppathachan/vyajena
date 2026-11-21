//! message-content: Group settings travel encrypted; User-owned profiles, and
//! client-runtime: Events in order, for what changes when devices come and go
//! (qa-fixes D2, D3, D5): devices added later learn the group name and the
//! profiles from re-sends that change nothing elsewhere, a reset user's old
//! profile is dropped, and `MembersChanged` names what changed and who did it.

mod support;

use client::{Event, Member, MessageBody, MessageId, SendStatus, UserId};
use support::Check;
use support::device::{self, Device, member, user};
use support::server::{ADMIN, TestServer};

async fn wait_accepted(device: &mut Device, id: MessageId) {
    device
        .wait_event(
            "accepted",
            |e| matches!(e, Event::SendStatus { id: i, status: SendStatus::Accepted } if *i == id),
        )
        .await;
}

/// Bob's phone links a new tablet and adds it.
async fn link_tablet(phone: &Device) -> Device {
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
    tablet
}

/// The admin calls themself "Akhil"; Bob calls himself "Bobby". Everyone has
/// seen it.
async fn named(admin: &mut Device, bob: &mut Device) {
    let id = admin
        .client
        .set_profile(Some("Akhil"), None)
        .check("profile");
    wait_accepted(admin, id).await;
    let id = bob.client.set_profile(Some("Bobby"), None).check("profile");
    wait_accepted(bob, id).await;
    bob.wait_until("Akhil's profile", |c| {
        c.display_name(&user(ADMIN))
            .is_ok_and(|n| n == "Akhil (akhil)")
    })
    .await;
    admin
        .wait_until("Bob's profile", |c| {
            c.display_name(&user("bob"))
                .is_ok_and(|n| n == "Bobby (bob)")
        })
        .await;
}

/// Waits until `device` shows the group name and both display names.
async fn learns_the_names(device: &Device) {
    device
        .wait_until("the profiles", |c| {
            c.display_name(&user(ADMIN))
                .is_ok_and(|n| n == "Akhil (akhil)")
                && c.display_name(&user("bob"))
                    .is_ok_and(|n| n == "Bobby (bob)")
        })
        .await;
}

fn changes_state(event: &Event) -> bool {
    matches!(event, Event::ProfileChanged { .. })
}

fn texts(device: &Device) -> Vec<MessageBody> {
    device
        .client
        .history(100)
        .check("history")
        .into_iter()
        .map(|m| m.body)
        .collect()
}

// spec: message-content/group-settings-travel-encrypted
// spec: message-content/user-owned-profiles
#[tokio::test(flavor = "multi_thread")]
async fn a_linked_tablet_learns_the_group_name_and_profiles() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let mut phone = device::member_of_group(&admin, "bob").await;
    phone.wait_members(&[member(&admin), member(&phone)]).await;
    named(&mut admin, &mut phone).await;
    // Skip what the naming produced.
    let _ = admin.sees_within_quiet(|_| false).await;
    let seen = (admin.events().len(), phone.events().len());

    let tablet = link_tablet(&phone).await;
    learns_the_names(&tablet).await;
    // Re-sends of equal values change nothing on the other devices.
    let _ = admin.sees_within_quiet(|_| false).await;
    for (device, from) in [(&admin, seen.0), (&phone, seen.1)] {
        let after: Vec<Event> = device.events()[from..].to_vec();
        assert!(!after.iter().any(changes_state), "{after:?}");
    }
    assert!(texts(&tablet).is_empty(), "re-sends are not chat messages");
}

// spec: message-content/group-settings-travel-encrypted
// spec: message-content/user-owned-profiles
#[tokio::test(flavor = "multi_thread")]
async fn a_newly_added_user_learns_the_group_name_and_profiles() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    bob.wait_members(&[member(&admin), member(&bob)]).await;
    named(&mut admin, &mut bob).await;
    let _ = bob.sees_within_quiet(|_| false).await;
    let seen = bob.events().len();

    let carol = device::member_of_group(&admin, "carol").await;
    learns_the_names(&carol).await;
    assert!(texts(&carol).is_empty());
    let _ = bob.sees_within_quiet(|_| false).await;
    let after: Vec<Event> = bob.events()[seen..].to_vec();
    assert!(!after.iter().any(changes_state), "{after:?}");
}

fn members_changed(added: &[Member], removed: &[Member], by: &UserId) -> Event {
    Event::MembersChanged {
        added: added.to_vec(),
        removed: removed.to_vec(),
        by: Some(by.clone()),
        automatic: Vec::new(),
    }
}

/// A `MembersChanged` with devices in it (not the one for creating or
/// joining the group).
fn is_members_changed(event: &Event) -> bool {
    matches!(event, Event::MembersChanged { added, removed, .. }
        if !added.is_empty() || !removed.is_empty())
}

// spec: client-runtime/events-in-order
#[tokio::test(flavor = "multi_thread")]
async fn members_changed_names_the_devices_and_the_committer() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let mut phone = device::member_of_group(&admin, "bob").await;
    let bob_added = members_changed(&[member(&phone)], &[], &user(ADMIN));
    assert_eq!(
        admin.wait_event("bob added", is_members_changed).await,
        bob_added
    );
    // The device that joined hears of no change for the commit adding it.
    phone
        .wait_event("joined", |e| {
            *e == Event::MembersChanged {
                added: Vec::new(),
                removed: Vec::new(),
                by: None,
                automatic: Vec::new(),
            }
        })
        .await;

    // Bob's phone commits the Add of Bob's tablet.
    let mut tablet = link_tablet(&phone).await;
    let tablet_added = members_changed(&[member(&tablet)], &[], &user("bob"));
    assert_eq!(
        admin.wait_event("tablet added", is_members_changed).await,
        tablet_added
    );
    assert_eq!(
        phone.wait_event("tablet added", is_members_changed).await,
        tablet_added
    );

    // Bob removes his tablet from his phone.
    let tablet_member = member(&tablet);
    phone
        .client
        .revoke_device(&tablet.device_id())
        .await
        .check("revoke the tablet");
    let tablet_removed = members_changed(&[], &[tablet_member], &user("bob"));
    assert_eq!(
        admin.wait_event("tablet removed", is_members_changed).await,
        tablet_removed
    );
    tablet
        .wait_event("removed", |e| matches!(e, Event::Removed))
        .await;

    // The admin removes Bob.
    let bob_member = member(&phone);
    admin
        .client
        .remove_user(&user("bob"))
        .await
        .check("remove bob");
    let bob_removed = members_changed(&[], &[bob_member], &user(ADMIN));
    assert_eq!(
        admin.wait_event("bob removed", is_members_changed).await,
        bob_removed
    );
}

// spec: message-content/user-owned-profiles
// spec: key-directory/clients-verify-the-identity-chain
#[tokio::test(flavor = "multi_thread")]
async fn a_reset_user_starts_without_a_name() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let mut alice = device::member_of_group(&admin, "alice").await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    let everyone = [member(&admin), member(&alice), member(&bob)];
    for device in [&admin, &alice, &bob] {
        device.wait_members(&everyone).await;
    }
    let id = bob.client.set_profile(Some("Bobby"), None).check("profile");
    wait_accepted(&mut bob, id).await;
    for device in [&admin, &alice] {
        device
            .wait_until("Bobby", |c| {
                c.display_name(&user("bob"))
                    .is_ok_and(|n| n == "Bobby (bob)")
            })
            .await;
    }

    // The admin resets Bob; Bob registers again with a new identity key.
    admin
        .client
        .remove_user(&user("bob"))
        .await
        .check("remove bob");
    drop(bob);
    let mut new_bob = device::invited(&admin, "bob").await;
    let refused = admin.client.add_user(&user("bob")).await;
    assert!(refused.is_err(), "{refused:?}");
    admin
        .wait_event(
            "the warning",
            |e| matches!(e, Event::IdentityKeyChanged { user: u } if *u == user("bob")),
        )
        .await;
    assert_eq!(admin.client.display_name(&user("bob")).check("name"), "bob");
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

    // Alice's core warns and drops the old profile at once.
    alice
        .wait_event(
            "the warning",
            |e| matches!(e, Event::IdentityKeyChanged { user: u } if *u == user("bob")),
        )
        .await;
    alice
        .wait_event("the profile dropped", |e| {
            *e == Event::ProfileChanged {
                user: user("bob"),
                display_name: None,
                about: None,
            }
        })
        .await;
    assert_eq!(alice.client.display_name(&user("bob")).check("name"), "bob");

    // A new profile comes from the new Bob's devices only.
    let id = new_bob
        .client
        .set_profile(Some("Robert"), None)
        .check("profile");
    wait_accepted(&mut new_bob, id).await;
    alice
        .wait_until("Robert", |c| {
            c.display_name(&user("bob"))
                .is_ok_and(|n| n == "Robert (bob)")
        })
        .await;
}

// spec: app-ui/chat-screen
#[tokio::test(flavor = "multi_thread")]
async fn messages_from_any_own_device_are_own_user() {
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let mut phone = device::member_of_group(&admin, "bob").await;
    let mut tablet = link_tablet(&phone).await;
    phone
        .wait_members(&[member(&admin), member(&phone), member(&tablet)])
        .await;

    let id = tablet.client.send_text("from the tablet").check("send");
    wait_accepted(&mut tablet, id).await;
    let received = phone
        .wait_event("the tablet's message", |e| matches!(e, Event::Message(_)))
        .await;
    let Event::Message(received) = received else {
        unreachable!()
    };
    assert!(received.own_user && !received.outgoing, "{received:?}");
    let own = tablet.client.message(id).check("message").check("stored");
    assert!(own.own_user && own.outgoing);
    let mut admin = admin;
    let seen = admin
        .wait_event("the tablet's message", |e| matches!(e, Event::Message(_)))
        .await;
    let Event::Message(seen) = seen else {
        unreachable!()
    };
    assert!(!seen.own_user && !seen.outgoing);
    let history = phone.client.history(10).check("history");
    assert!(history.iter().all(|m| m.own_user), "{history:?}");
}
