//! Chatting through the bridge (flutter-app task 2.3): send, send status,
//! events in the core's order, history, the composer limit, profiles and
//! members, admin-only calls.

mod support;

use bridge::api::core::text_fits;
use bridge::api::error::CoreErrorKind;
use bridge::api::types::{ChatBody, ChatMessage, CoreEvent, SendState};
use client::messaging::MAX_CONTENT_BYTES;
use support::Check;
use support::device::{self, Device, user};
use support::pi::{ADMIN, TestPi};

fn text_of(message: &ChatMessage) -> &str {
    match &message.body {
        ChatBody::Text { text } => text,
        ChatBody::Unsupported => "<unsupported>",
    }
}

async fn wait_text(device: &mut Device, text: &str) -> ChatMessage {
    let event = device
        .wait_event(
            &format!("the message {text:?}"),
            |e| matches!(e, CoreEvent::Message { message } if text_of(message) == text),
        )
        .await;
    match event {
        CoreEvent::Message { message } => message,
        _ => unreachable!(),
    }
}

async fn wait_accepted(device: &mut Device, id: i64) {
    device
        .wait_event("accepted", |e| {
            matches!(e, CoreEvent::SendStatus { id: i, status: SendState::Accepted } if *i == id)
        })
        .await;
}

fn history(device: &Device) -> Vec<String> {
    device
        .core
        .history(100)
        .check("history")
        .iter()
        .map(|m| text_of(m).to_owned())
        .collect()
}

// spec: app-ui/chat-screen
#[tokio::test(flavor = "multi_thread")]
async fn alice_and_bob_chat_and_alice_sees_hi_go_from_sending_to_accepted() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    let mut alice = device::member_of_group(&admin, "alice").await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    alice
        .wait_until("alice sees bob", |c| {
            c.members()
                .is_ok_and(|m| m.iter().any(|d| d.user == user("bob")))
        })
        .await;

    // Offline, the message stays sending (⏳) until the core can send it.
    alice.core.stop();
    let hi = alice.core.send_text("hi".to_owned()).check("send hi");
    let sending = alice.core.message(hi).check("message").check("stored");
    assert!(sending.outgoing);
    assert_eq!(sending.status, Some(SendState::Sending));
    assert_eq!(sending.accepted_at_ms, None);
    alice.core.start().await.check("restart");
    wait_accepted(&mut alice, hi).await;
    let accepted = alice.core.message(hi).check("message").check("stored");
    assert_eq!(accepted.status, Some(SendState::Accepted));
    assert!(accepted.accepted_at_ms.is_some());

    let received = wait_text(&mut bob, "hi").await;
    assert_eq!(received.sender, user("alice"));
    assert!(!received.outgoing);
    assert_eq!(received.status, None);
    assert_eq!(
        Some(received.sender_device),
        alice.core.device_id().check("device id")
    );

    let hey = bob.core.send_text("hey".to_owned()).check("send hey");
    wait_accepted(&mut bob, hey).await;
    wait_text(&mut alice, "hey").await;
    assert_eq!(history(&alice), ["hi", "hey"]);
    assert_eq!(history(&bob), ["hi", "hey"]);
}

// spec: client-runtime/events-in-order
#[tokio::test(flavor = "multi_thread")]
async fn messages_reach_the_bridge_in_the_core_order() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    let words = ["one", "two", "three", "four", "five"];
    for word in words {
        admin.core.send_text(word.to_owned()).check("send");
    }
    for word in words {
        wait_text(&mut bob, word).await;
    }
    let received: Vec<_> = bob
        .events()
        .into_iter()
        .filter_map(|e| match e {
            CoreEvent::Message { message } => Some(message),
            _ => None,
        })
        .collect();
    let texts: Vec<_> = received.iter().map(text_of).collect();
    assert_eq!(texts, words);
    let accepted: Vec<_> = received.iter().map(|m| m.accepted_at_ms).collect();
    assert!(accepted.windows(2).all(|w| w[0] <= w[1]), "{accepted:?}");
    // The sender sees each of its own messages accepted, in order too.
    let mut ids = Vec::new();
    for event in admin.events() {
        if let CoreEvent::SendStatus {
            id,
            status: SendState::Accepted,
        } = event
        {
            ids.push(id);
        }
    }
    assert_eq!(ids.len(), words.len());
    assert!(ids.windows(2).all(|w| w[0] < w[1]), "{ids:?}");
    assert_eq!(history(&admin), words);
}

// spec: app-ui/composer-limit
#[tokio::test(flavor = "multi_thread")]
async fn text_over_the_limit_is_refused_before_sending() {
    assert!(text_fits("hi".to_owned()));
    let long = "x".repeat(MAX_CONTENT_BYTES + 1);
    assert!(!text_fits(long.clone()));

    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    let error = admin.core.send_text(long).expect_err("too long");
    assert_eq!(error.kind, CoreErrorKind::TooLong);
    assert!(history(&admin).is_empty());
}

// spec: app-ui/profile-and-members
#[tokio::test(flavor = "multi_thread")]
async fn alice_renames_herself_and_bob_sees_ali_alice() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    let alice = device::member_of_group(&admin, "alice").await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    alice
        .wait_until("alice sees bob", |c| {
            c.members()
                .is_ok_and(|m| m.iter().any(|d| d.user == user("bob")))
        })
        .await;
    alice
        .core
        .set_profile(Some("Ali".to_owned()), Some("hello".to_owned()))
        .check("set profile");
    bob.wait_event("alice's profile", |e| {
        matches!(e, CoreEvent::ProfileChanged { user: u, display_name: Some(n), .. }
            if *u == user("alice") && n == "Ali")
    })
    .await;
    assert_eq!(
        bob.core.display_name(user("alice")).check("display name"),
        "Ali (alice)"
    );
    let profile = bob.core.profile(user("alice")).check("profile");
    assert_eq!(profile.about.as_deref(), Some("hello"));

    let members = bob.core.members().check("members");
    let users: std::collections::BTreeSet<_> =
        members.iter().map(|m| m.user.username.clone()).collect();
    assert_eq!(
        users,
        [ADMIN, "alice", "bob"]
            .map(str::to_owned)
            .into_iter()
            .collect()
    );
    assert!(members.iter().all(|m| m.device.len() == 32));
}

// spec: app-ui/admin-screens
#[tokio::test(flavor = "multi_thread")]
async fn admin_calls_work_for_the_admin_and_not_for_bob() {
    let pi = TestPi::start().await;
    let mut admin = device::admin_with_group(&pi).await;
    let bob = device::member_of_group(&admin, "bob").await;

    let invite = admin
        .core
        .create_invite("carol".to_owned())
        .await
        .check("invite carol");
    let listed = admin.core.list_invites().await.check("list invites");
    let carol = listed
        .iter()
        .find(|i| i.code == invite.code)
        .check("carol's invite listed");
    assert_eq!(carol.user, user("carol"));
    assert_eq!(carol.text, None);
    assert_eq!(carol.expires_at_ms, invite.expires_at_ms);
    admin
        .core
        .revoke_invite(invite.code.clone())
        .await
        .check("revoke");
    let listed = admin.core.list_invites().await.check("list invites");
    assert!(listed.iter().all(|i| i.code != invite.code));

    let id = admin
        .core
        .set_group_name(Some("Family".to_owned()))
        .check("group name");
    wait_accepted(&mut admin, id).await;
    bob.wait_until("bob sees the group name", |c| {
        c.group_name().ok().flatten().as_deref() == Some("Family")
    })
    .await;

    let error = bob
        .core
        .create_invite("dave".to_owned())
        .await
        .expect_err("bob is not the admin");
    assert_eq!(error.kind, CoreErrorKind::Refused, "{error:?}");
    let error = bob
        .core
        .set_group_name(Some("Mine".to_owned()))
        .expect_err("bob is not the admin");
    assert_eq!(error.kind, CoreErrorKind::NotAdmin);

    admin
        .core
        .remove_user(user("bob"))
        .await
        .check("remove bob");
    admin
        .wait_until("bob removed", |c| {
            c.members()
                .is_ok_and(|m| m.iter().all(|d| d.user != user("bob")))
        })
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_device_has_no_history_and_refuses_a_malformed_device_id() {
    let device = Device::new().await;
    assert_eq!(device.core.history(10).check("history"), Vec::new());
    assert!(!device.core.is_connected());
    let error = device
        .core
        .revoke_device("zz".to_owned())
        .await
        .expect_err("bad id");
    assert_eq!(error.kind, CoreErrorKind::InvalidPayload);
    let error = device
        .core
        .send_text("hi".to_owned())
        .expect_err("no account");
    assert_eq!(error.kind, CoreErrorKind::NotRegistered);
}
