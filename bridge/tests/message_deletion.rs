//! Deleting through the bridge (change `delete-messages`): an admin delete in
//! the group shows a tombstone "by the admin", and a DM delete leaves a
//! reveal-able tombstone (message-deletion).

mod support;

use bridge::api::types::{ChatBody, ChatMessage, CoreEvent, SendState};
use support::Check;
use support::device::{self, Device, user};
use support::server::TestServer;

async fn wait_text(device: &mut Device, text: &str) -> ChatMessage {
    let event = device
        .wait_event(&format!("the message {text:?}"), |e| {
            matches!(e, CoreEvent::Message { message }
                if matches!(&message.body, ChatBody::Text { text: t } if t == text))
        })
        .await;
    match event {
        CoreEvent::Message { message } => message,
        _ => unreachable!(),
    }
}

async fn wait_deleted(device: &mut Device, id: i64) -> (bool, bool) {
    let event = device
        .wait_event("a tombstone", |e| {
            matches!(e, CoreEvent::Message { message }
                if message.id == id && matches!(message.body, ChatBody::Deleted { .. }))
        })
        .await;
    match event {
        CoreEvent::Message { message } => match message.body {
            ChatBody::Deleted {
                by_admin,
                revealable,
            } => (by_admin, revealable),
            _ => unreachable!(),
        },
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

async fn trio(server: &TestServer) -> (Device, Device, Device) {
    let admin = device::admin_with_group(server).await;
    let mut alice = device::member_of_group(&admin, "alice").await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    for device in [&mut alice, &mut bob] {
        device
            .wait_until("the whole group", |c| {
                c.members().is_ok_and(|m| m.len() == 3)
            })
            .await;
    }
    (admin, alice, bob)
}

// spec: app-ui/deleting-a-message
#[tokio::test(flavor = "multi_thread")]
async fn an_admin_delete_tombstones_by_the_admin_through_the_bridge() {
    let server = TestServer::start().await;
    let (mut admin, mut alice, mut bob) = trio(&server).await;

    let id = alice.core.send_text("doomed".into()).check("send");
    wait_accepted(&mut alice, id).await;
    let admin_msg = wait_text(&mut admin, "doomed").await;
    let bob_msg = wait_text(&mut bob, "doomed").await;

    admin
        .core
        .delete_message(0, admin_msg.id)
        .check("admin delete");
    let (by_admin, revealable) = wait_deleted(&mut bob, bob_msg.id).await;
    assert!(by_admin);
    assert!(!revealable);
}

// spec: app-ui/deleting-a-message
#[tokio::test(flavor = "multi_thread")]
async fn a_dm_delete_reveals_through_the_bridge() {
    let server = TestServer::start().await;
    let (_admin, mut alice, mut bob) = trio(&server).await;

    let chat = alice.core.open_dm(user("bob")).await.check("open DM");
    let id = alice
        .core
        .send_message_in(chat, "secret".into(), vec![], None)
        .check("send in DM");
    wait_accepted(&mut alice, id).await;
    let bob_msg = wait_text(&mut bob, "secret").await;

    alice.core.delete_message(chat, id).check("delete own");
    let (by_admin, revealable) = wait_deleted(&mut bob, bob_msg.id).await;
    assert!(!by_admin);
    assert!(revealable);

    let revealed = bob.core.reveal(bob_msg.chat_id, bob_msg.id).check("reveal");
    assert!(matches!(&revealed.body, ChatBody::Text { text } if text == "secret"));
}
