//! local-notifications end to end against the real server (change
//! `smarter-notifications`): the summary the device builds after it catches up
//! (type and unread count, never a sender or text), the type from the wire
//! `group_id` alone, an unread-based count, the admin-event set and its mute
//! gate, and the per-device "Notification detail" level.
//!
//! Cast: the admin (`akhil`), Alice, Bob and Carol, all in the lobby.

mod support;

use client::{
    AdminEventKind, ChatId, ChatKind, ConversationKind, Event, MessageBody, MuteTarget,
    NotificationDetail,
};
use support::Check;
use support::device::{self, Device, member, user};
use support::server::TestServer;

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
    device
        .client
        .chats()
        .check("chats")
        .into_iter()
        .find(|c| matches!(&c.kind, ChatKind::Group { name: Some(n), .. } if n == name))
        .check("the group")
        .id
}

async fn wait_text(device: &mut Device, text: &str) {
    let wanted = MessageBody::Text(text.to_owned());
    device
        .wait_event(
            &format!("the message {text:?}"),
            |e| matches!(e, Event::Message(m) if m.body == wanted),
        )
        .await;
}

/// Waits until `device`'s summary satisfies `done`.
async fn wait_summary(
    device: &Device,
    what: &str,
    done: impl Fn(&client::NotificationSummary) -> bool,
) {
    device
        .wait_until(what, |c| c.notification_summary().is_ok_and(|s| done(&s)))
        .await;
}

// spec: local-notifications/the-device-builds-the-notification-locally
#[tokio::test(flavor = "multi_thread")]
async fn bob_catches_up_and_the_summary_has_a_type_and_a_count_and_nothing_else() {
    let server = TestServer::start().await;
    let (admin, mut alice, mut bob, _carol) = lobby(&server).await;
    let work = admin
        .client
        .create_named_group(Some("Work"), &[user("alice"), user("bob")])
        .await
        .check("create Work");
    let alice_work = wait_group(&alice, "Work").await;
    wait_group(&bob, "Work").await;
    for n in 1..=3 {
        alice
            .client
            .send_message_in(alice_work, &format!("secret plan {n}"), &[], None)
            .check("send in Work");
    }
    // A DM with two unread messages as well.
    let dm = alice
        .client
        .open_dm(&user("bob"))
        .await
        .check("open the DM");
    alice
        .client
        .send_message_in(dm, "private hello", &[], None)
        .check("DM 1");
    alice
        .client
        .send_message_in(dm, "private again", &[], None)
        .check("DM 2");
    wait_text(&mut bob, "secret plan 3").await;
    wait_text(&mut bob, "private again").await;
    let _ = (&mut alice, work);

    let summary = bob.client.notification_summary().check("summary");
    let work_line = summary
        .conversations
        .iter()
        .find(|c| c.kind == ConversationKind::NamedGroup)
        .check("the Work line");
    assert_eq!(work_line.name.as_deref(), Some("Work"));
    assert_eq!(work_line.unread, 3);
    let dm_line = summary
        .conversations
        .iter()
        .find(|c| c.kind == ConversationKind::Dm)
        .check("the DM line");
    // A DM names no one: the peer's name would be a sender.
    assert_eq!(dm_line.name, None);
    assert_eq!(dm_line.unread, 2);
    assert_eq!(summary.total_unread, 5);

    // Nothing decrypted reaches it: no body, no sender, no caption.
    let shown = format!("{summary:?}").to_lowercase();
    for leaked in ["secret", "private", "alice", "hello"] {
        assert!(!shown.contains(leaked), "{leaked} leaked into {shown}");
    }
}

// spec: local-notifications/conversation-type-without-decryption
#[tokio::test(flavor = "multi_thread")]
async fn a_group_id_alone_gives_the_conversation_type() {
    let server = TestServer::start().await;
    let (admin, alice, bob, _carol) = lobby(&server).await;
    admin
        .client
        .create_named_group(Some("Work"), &[user("alice"), user("bob")])
        .await
        .check("create Work");
    let work = wait_group(&bob, "Work").await;
    let dm = alice
        .client
        .open_dm(&user("bob"))
        .await
        .check("open the DM");
    bob.wait_until("the DM", |c| {
        c.chats()
            .is_ok_and(|chats| chats.iter().any(|c| matches!(c.kind, ChatKind::Dm { .. })))
    })
    .await;
    let bob_dm = bob
        .client
        .chats()
        .check("chats")
        .into_iter()
        .find(|c| matches!(c.kind, ChatKind::Dm { .. }))
        .check("Bob's DM")
        .id;
    let _ = dm;

    // No message was sent anywhere, so nothing could have been decrypted: the
    // type comes from the group id and the local record alone.
    let kind = |chat| {
        let group_id = bob.client.chat_group_id(chat).check("group id");
        bob.client
            .conversation_kind_of_group(&group_id)
            .check("kind")
    };
    assert_eq!(kind(ChatId::MAIN), Some(ConversationKind::Lobby));
    assert_eq!(kind(work), Some(ConversationKind::NamedGroup));
    assert_eq!(kind(bob_dm), Some(ConversationKind::Dm));
    assert_eq!(
        bob.client
            .conversation_kind_of_group(b"no-such-group")
            .check("unknown"),
        None
    );
}

// spec: local-notifications/unread-count-not-push-count
#[tokio::test(flavor = "multi_thread")]
async fn the_count_is_the_unread_state_however_often_the_device_wakes() {
    let server = TestServer::start().await;
    let (mut admin, _alice, mut bob, _carol) = lobby(&server).await;
    for n in 1..=3 {
        admin.client.send_text(&format!("news {n}")).check("send");
    }
    wait_text(&mut bob, "news 3").await;
    let _ = &mut admin;

    // Three wakes (some collapsed or duplicated by the platform) and the core
    // still says exactly what it holds as unread.
    for _ in 0..3 {
        let summary = bob.client.notification_summary().check("summary");
        assert_eq!(summary.conversations.len(), 1);
        assert_eq!(summary.conversations[0].kind, ConversationKind::Lobby);
        assert_eq!(summary.conversations[0].unread, 3);
    }
    bob.client.mark_chat_read(ChatId::MAIN).check("read");
    assert!(
        bob.client
            .notification_summary()
            .check("summary")
            .conversations
            .is_empty()
    );
    admin.client.send_text("one more").check("send");
    wait_text(&mut bob, "one more").await;
    let summary = bob.client.notification_summary().check("summary");
    assert_eq!(summary.conversations[0].unread, 1);
}

// spec: local-notifications/admin-event-notifications-are-fixed-content-free-phrases
#[tokio::test(flavor = "multi_thread")]
async fn added_removed_and_closed_are_admin_events_and_routine_churn_is_not() {
    let server = TestServer::start().await;
    let (admin, alice, bob, _carol) = lobby(&server).await;
    let chat = admin
        .client
        .create_named_group(Some("Weekend"), &[user("alice"), user("bob")])
        .await
        .check("create the group");
    wait_group(&bob, "Weekend").await;
    wait_group(&alice, "Weekend").await;

    // Added to a new named group.
    wait_summary(&bob, "Bob's added event", |s| {
        s.admin_events
            .iter()
            .any(|e| e.kind == AdminEventKind::Added && e.name.as_deref() == Some("Weekend"))
    })
    .await;
    let events = bob
        .client
        .notification_summary()
        .check("summary")
        .admin_events;
    bob.client
        .clear_admin_events(events.iter().map(|e| e.id).max().unwrap_or(0))
        .check("clear");
    assert!(
        bob.client
            .notification_summary()
            .check("summary")
            .admin_events
            .is_empty()
    );
    let alice_events = alice
        .client
        .notification_summary()
        .check("summary")
        .admin_events;
    alice
        .client
        .clear_admin_events(alice_events.iter().map(|e| e.id).max().unwrap_or(0))
        .check("clear");

    // Routine churn: Carol joins the group. Alice and Bob are told in-chat only.
    admin
        .client
        .add_to_group(chat, &user("carol"))
        .await
        .check("add Carol");
    admin
        .wait_until("Carol in the group", |c| {
            c.group_members(chat)
                .is_ok_and(|m| m.iter().any(|m| m.user == user("carol")))
        })
        .await;
    alice
        .wait_until("Alice sees Carol", |c| {
            c.group_members(chat)
                .is_ok_and(|m| m.iter().any(|m| m.user == user("carol")))
        })
        .await;
    assert!(
        alice
            .client
            .notification_summary()
            .check("summary")
            .admin_events
            .is_empty()
    );

    // The admin removes Bob: Bob's own access changed.
    admin
        .client
        .remove_from_group(chat, &user("bob"))
        .await
        .check("remove Bob");
    wait_summary(&bob, "Bob's removed event", |s| {
        s.admin_events
            .iter()
            .any(|e| e.kind == AdminEventKind::Removed && e.name.as_deref() == Some("Weekend"))
    })
    .await;
    // Alice, who stayed, has none for it.
    assert!(
        alice
            .client
            .notification_summary()
            .check("summary")
            .admin_events
            .iter()
            .all(|e| e.kind != AdminEventKind::Removed)
    );

    // The admin closes it: Alice is told; the admin who did it is not.
    admin.client.close_group(chat).await.check("close");
    wait_summary(&alice, "Alice's closed event", |s| {
        s.admin_events
            .iter()
            .any(|e| e.kind == AdminEventKind::Closed)
    })
    .await;
    assert!(
        admin
            .client
            .notification_summary()
            .check("summary")
            .admin_events
            .is_empty()
    );
}

// spec: local-notifications/the-device-builds-the-notification-locally
#[tokio::test(flavor = "multi_thread")]
async fn a_muted_conversation_and_whole_app_mute_gate_the_local_build() {
    // design D8: the local build re-checks the core's mute view, so a conversation
    // muted after a wake was already in flight raises no notification.
    let server = TestServer::start().await;
    let (mut admin, _alice, mut bob, _carol) = lobby(&server).await;
    admin.client.send_text("hello").check("send");
    wait_text(&mut bob, "hello").await;
    let _ = &mut admin;
    assert_eq!(
        bob.client
            .notification_summary()
            .check("summary")
            .conversations
            .len(),
        1
    );

    bob.client
        .set_mute_pref(MuteTarget::Conversation(ChatId::MAIN), true)
        .await
        .check("mute the lobby");
    let summary = bob.client.notification_summary().check("summary");
    assert!(summary.conversations.is_empty());
    assert_eq!(summary.total_unread, 0);

    bob.client
        .set_mute_pref(MuteTarget::Conversation(ChatId::MAIN), false)
        .await
        .check("unmute");
    bob.client
        .set_mute_pref(MuteTarget::Global, true)
        .await
        .check("mute all");
    let summary = bob.client.notification_summary().check("summary");
    assert!(summary.conversations.is_empty() && summary.admin_events.is_empty());
}

// spec: local-notifications/respect-the-notification-detail-level
#[tokio::test(flavor = "multi_thread")]
async fn the_detail_level_defaults_to_basic_and_survives_a_restart() {
    let server = TestServer::start().await;
    let (_admin, _alice, mut bob, _carol) = lobby(&server).await;
    assert_eq!(
        bob.client.notification_detail().check("detail"),
        NotificationDetail::Basic
    );
    bob.client
        .set_notification_detail(NotificationDetail::More)
        .check("set More");
    assert_eq!(
        bob.client.notification_detail().check("detail"),
        NotificationDetail::More
    );
    bob.restart().await;
    assert_eq!(
        bob.client.notification_detail().check("detail"),
        NotificationDetail::More
    );
    bob.client
        .set_notification_detail(NotificationDetail::Basic)
        .check("set Basic");
    assert_eq!(
        bob.client.notification_detail().check("detail"),
        NotificationDetail::Basic
    );
}
