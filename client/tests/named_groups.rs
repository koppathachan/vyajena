//! Named groups end to end against the real server (change
//! `lobby-multi-group`): the lobby's fixed name, creating a named group with a
//! subset of the members, its name, membership, closing, and removing a user
//! from every group.
//!
//! Cast: the admin (`akhil`), Alice, Bob and Carol, all in the lobby.

mod support;

use client::{
    Chat, ChatId, ChatKind, ClientError, Event, Message, MessageBody, MessageId, SendStatus,
};
use support::Check;
use support::device::{self, Device, member, user};
use support::server::TestServer;

/// The admin, Alice, Bob and Carol, all in the lobby and seeing each other.
async fn lobby(server: &TestServer) -> (Device, Device, Device, Device) {
    let admin = device::admin_with_group(server).await;
    let alice = device::member_of_group(&admin, "alice").await;
    let bob = device::member_of_group(&admin, "bob").await;
    let carol = device::member_of_group(&admin, "carol").await;
    let everyone = [member(&admin), member(&alice), member(&bob), member(&carol)];
    for device in [&admin, &alice, &bob, &carol] {
        device.wait_members(&everyone).await;
    }
    (admin, alice, bob, carol)
}

async fn wait_text(device: &mut Device, text: &str) -> Message {
    let wanted = MessageBody::Text(text.to_owned());
    match device
        .wait_event(
            &format!("the message {text:?}"),
            |e| matches!(e, Event::Message(m) if m.body == wanted),
        )
        .await
    {
        Event::Message(message) => message,
        _ => unreachable!(),
    }
}

async fn wait_accepted(device: &mut Device, id: MessageId) {
    device
        .wait_event(
            "the message accepted",
            |e| matches!(e, Event::SendStatus { id: i, status: SendStatus::Accepted } if *i == id),
        )
        .await;
}

fn group_named(device: &Device, wanted: &str) -> Option<Chat> {
    device
        .client
        .chats()
        .check("chats")
        .into_iter()
        .find(|c| matches!(&c.kind, ChatKind::Group { name: Some(n), .. } if n == wanted))
}

/// Waits until `device` has a named group called `name` and returns it.
async fn wait_group(device: &Device, name: &str) -> ChatId {
    device
        .wait_until("the named group", |c| {
            c.chats().is_ok_and(|chats| {
                chats.iter().any(
                    |chat| matches!(&chat.kind, ChatKind::Group { name: Some(n), .. } if n == name),
                )
            })
        })
        .await;
    group_named(device, name).check("the group").id
}

/// Waits until the admin's view of group `chat` lists exactly `users`' devices.
async fn wait_members_of(admin: &Device, chat: ChatId, who: &[&Device]) {
    let mut wanted: Vec<_> = who.iter().map(|d| member(d)).collect();
    wanted.push(member(admin));
    wanted.sort();
    admin
        .wait_until("the group's members", |c| {
            c.group_members(chat).is_ok_and(|m| m == wanted)
        })
        .await;
}

// spec: multi-group/the-system-lobby
#[tokio::test(flavor = "multi_thread")]
async fn the_existing_group_is_the_lobby_and_has_no_rename() {
    let server = TestServer::start().await;
    let (admin, alice, _bob, _carol) = lobby(&server).await;
    for device in [&admin, &alice] {
        let chats = device.client.chats().check("chats");
        assert_eq!(chats[0].id, ChatId::MAIN);
        assert_eq!(chats[0].kind, ChatKind::Lobby);
    }
    assert!(admin.client.named_groups_available());
    // The lobby can be neither renamed nor closed through the API.
    let renamed = admin.client.rename_group(ChatId::MAIN, "Home chat");
    assert!(
        matches!(renamed, Err(ClientError::NoSuchChat)),
        "{renamed:?}"
    );
    let closed = admin.client.close_group(ChatId::MAIN).await;
    assert!(matches!(closed, Err(ClientError::NoSuchChat)), "{closed:?}");
}

// spec: multi-group/named-group-creation
// spec: multi-group/named-group-membership
// spec: message-content/group-settings-travel-encrypted
#[tokio::test(flavor = "multi_thread")]
async fn the_admin_creates_a_group_with_a_subset_and_it_is_named() {
    let server = TestServer::start().await;
    let (mut admin, mut alice, mut bob, mut carol) = lobby(&server).await;

    let chat = admin
        .client
        .create_named_group(Some("Weekend"), &[user("alice"), user("bob")])
        .await
        .check("create the group");
    assert!(!chat.is_main());
    wait_members_of(&admin, chat, &[&alice, &bob]).await;

    // Members show its name; Carol, not picked, has no such chat.
    let alice_chat = wait_group(&alice, "Weekend").await;
    let bob_chat = wait_group(&bob, "Weekend").await;
    assert!(group_named(&carol, "Weekend").is_none());
    assert!(
        carol
            .client
            .chats()
            .check("chats")
            .iter()
            .all(|c| !matches!(c.kind, ChatKind::Group { .. }))
    );

    // Messages reach only the group; the lobby holds none of it.
    let id = alice
        .client
        .send_message_in(alice_chat, "weekend plans", &[], None)
        .check("send in the group");
    wait_accepted(&mut alice, id).await;
    let got = wait_text(&mut bob, "weekend plans").await;
    assert_eq!(got.chat, bob_chat);
    wait_text(&mut admin, "weekend plans").await;
    assert!(
        !carol
            .sees_within_quiet(|e| matches!(e, Event::Message(m) if m.body == MessageBody::Text("weekend plans".into())))
            .await
    );
    assert!(carol.client.history(20).check("lobby").is_empty());

    // A non-admin cannot rename, add or close.
    let refused = bob.client.rename_group(bob_chat, "Bob's club");
    assert!(matches!(refused, Err(ClientError::NotAdmin)), "{refused:?}");
    let refused = bob.client.add_to_group(bob_chat, &user("carol")).await;
    assert!(matches!(refused, Err(ClientError::NotAdmin)), "{refused:?}");

    // The admin renames it; everyone shows the new name.
    admin
        .client
        .rename_group(chat, "Weekend away")
        .check("rename");
    wait_group(&alice, "Weekend away").await;
    wait_group(&bob, "Weekend away").await;

    // The admin adds Carol later, and she learns the current name.
    admin
        .client
        .add_to_group(chat, &user("carol"))
        .await
        .check("add Carol");
    wait_group(&carol, "Weekend away").await;
    wait_members_of(&admin, chat, &[&alice, &bob, &carol]).await;
    let _ = carol;
}

// spec: group-membership/automatic-add-of-new-users
#[tokio::test(flavor = "multi_thread")]
async fn a_new_member_joins_the_lobby_and_no_named_group() {
    let server = TestServer::start().await;
    let (admin, alice, _bob, _carol) = lobby(&server).await;
    admin
        .client
        .create_named_group(Some("Weekend"), &[user("alice")])
        .await
        .check("create the group");
    wait_group(&alice, "Weekend").await;

    let dan = device::member_of_group(&admin, "dan").await;
    let chats = dan.client.chats().check("chats");
    assert_eq!(chats.len(), 1, "{chats:?}");
    assert_eq!(chats[0].kind, ChatKind::Lobby);
}

// spec: multi-group/named-group-membership
#[tokio::test(flavor = "multi_thread")]
async fn a_removed_member_leaves_the_group_and_stays_in_the_lobby() {
    let server = TestServer::start().await;
    let (mut admin, alice, mut bob, _carol) = lobby(&server).await;
    let chat = admin
        .client
        .create_named_group(Some("Weekend"), &[user("alice"), user("bob")])
        .await
        .check("create the group");
    wait_group(&bob, "Weekend").await;
    wait_members_of(&admin, chat, &[&alice, &bob]).await;

    admin
        .client
        .remove_from_group(chat, &user("bob"))
        .await
        .check("remove Bob");
    wait_members_of(&admin, chat, &[&alice]).await;
    // Bob is still in the lobby and hears the admin there.
    let id = admin.client.send_text("lobby news").check("send");
    wait_accepted(&mut admin, id).await;
    wait_text(&mut bob, "lobby news").await;
}

// spec: multi-group/closing-a-named-group
#[tokio::test(flavor = "multi_thread")]
async fn closing_a_group_makes_it_read_only_for_everyone() {
    let server = TestServer::start().await;
    let (mut admin, mut alice, bob, _carol) = lobby(&server).await;
    let chat = admin
        .client
        .create_named_group(Some("Weekend"), &[user("alice"), user("bob")])
        .await
        .check("create the group");
    let alice_chat = wait_group(&alice, "Weekend").await;
    wait_group(&bob, "Weekend").await;
    wait_members_of(&admin, chat, &[&alice, &bob]).await;
    let id = alice
        .client
        .send_message_in(alice_chat, "before", &[], None)
        .check("send");
    wait_accepted(&mut alice, id).await;
    wait_text(&mut admin, "before").await;

    admin.client.close_group(chat).await.check("close");
    for device in [&admin, &alice, &bob] {
        device
            .wait_until("the group closed", |c| {
                c.chats().is_ok_and(|chats| {
                    chats.iter().any(|chat| {
                        matches!(&chat.kind, ChatKind::Group { name: Some(n), closed: true } if n == "Weekend")
                    })
                })
            })
            .await;
    }
    // History stays; sending is refused.
    let history = alice.client.history_in(alice_chat, 10).check("history");
    assert_eq!(history.len(), 1);
    let refused = alice.client.send_message_in(alice_chat, "after", &[], None);
    assert!(
        matches!(refused, Err(ClientError::ChatClosed)),
        "{refused:?}"
    );
    // A closed group closes once: closing again changes nothing.
    admin.client.close_group(chat).await.check("close again");
}

// spec: group-membership/removing-a-user
#[tokio::test(flavor = "multi_thread")]
async fn removing_a_user_removes_them_from_every_group() {
    let server = TestServer::start().await;
    let (admin, alice, bob, _carol) = lobby(&server).await;
    let chat = admin
        .client
        .create_named_group(Some("Weekend"), &[user("alice"), user("bob")])
        .await
        .check("create the group");
    wait_group(&bob, "Weekend").await;
    wait_members_of(&admin, chat, &[&alice, &bob]).await;

    admin
        .client
        .remove_user(&user("bob"))
        .await
        .check("remove Bob");
    wait_members_of(&admin, chat, &[&alice]).await;
    admin
        .wait_until("the lobby without Bob", |c| {
            c.members()
                .is_ok_and(|m| m.iter().all(|m| m.user != user("bob")))
        })
        .await;
}

// spec: multi-group/named-groups-reach-only-v3-apps
#[tokio::test(flavor = "multi_thread")]
async fn every_key_package_of_a_current_core_lists_the_named_extension() {
    let server = TestServer::start().await;
    let (_admin, _alice, bob, _carol) = lobby(&server).await;
    let id = bob.device_id();
    let (capable, total): (i64, i64) = server
        .query(move |db| {
            db.query_row(
                "SELECT sum(named_group_capable), count(*) FROM key_packages WHERE device_id = ?1",
                [id.as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })
        .await;
    assert!(total > 0);
    assert_eq!(capable, total);
}

// spec: message-content/group-settings-travel-encrypted
// spec: multi-group/named-group-membership
#[tokio::test(flavor = "multi_thread")]
async fn a_linked_tablet_joins_the_group_and_learns_its_name() {
    let server = TestServer::start().await;
    let (admin, alice, bob, _carol) = lobby(&server).await;
    admin
        .client
        .create_named_group(Some("Weekend"), &[user("alice"), user("bob")])
        .await
        .check("create the group");
    wait_group(&bob, "Weekend").await;
    wait_group(&alice, "Weekend").await;

    // Bob's phone links a tablet: the lobby, then the group, and its name.
    let mut tablet = Device::new().await;
    let request = tablet.client.link_request().await.check("link request");
    let approval = bob.client.approve_link(&request).await.check("approve");
    tablet
        .client
        .complete_link(&approval)
        .await
        .check("complete");
    tablet.client.start().await.check("start tablet");
    tablet
        .wait_event("the tablet joined", |e| matches!(e, Event::JoinedGroup))
        .await;
    wait_group(&tablet, "Weekend").await;
}
