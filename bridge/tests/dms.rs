//! DMs through the bridge (change `dms`): the chat list, opening a DM,
//! sending and reading in it, and the read mark.

mod support;

use bridge::api::error::CoreErrorKind;
use bridge::api::types::{ChatBody, ChatKindInfo, ChatMessage, CoreEvent, SendState};
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

async fn wait_accepted(device: &mut Device, id: i64) {
    device
        .wait_event("accepted", |e| {
            matches!(e, CoreEvent::SendStatus { id: i, status: SendState::Accepted } if *i == id)
        })
        .await;
}

// spec: app-ui/chat-list
// spec: app-ui/open-a-dm
#[tokio::test(flavor = "multi_thread")]
async fn alice_opens_a_dm_with_bob_through_the_bridge() {
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let mut alice = device::member_of_group(&admin, "alice").await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    alice
        .wait_until("alice sees bob", |c| {
            c.members()
                .is_ok_and(|m| m.iter().any(|d| d.user == user("bob")))
        })
        .await;
    assert!(alice.core.dms_available());
    assert_eq!(alice.core.chats().check("chats").len(), 1);

    let chat = alice.core.open_dm(user("bob")).await.check("open DM");
    assert_ne!(chat, 0);
    let id = alice
        .core
        .send_message_in(chat, "hi bob".into(), vec![], None)
        .check("send");
    wait_accepted(&mut alice, id).await;

    let got = wait_text(&mut bob, "hi bob").await;
    assert_ne!(got.chat_id, 0);
    let chats = bob.core.chats().check("chats");
    assert_eq!(chats.len(), 2);
    assert_eq!(chats[0].chat_id, 0);
    assert!(
        matches!(&chats[1].kind, ChatKindInfo::Dm { peer, closed: false, .. }
        if *peer == user("alice"))
    );
    assert_eq!(chats[1].unread, 1);
    let history = bob.core.history_in(got.chat_id, 10).check("history");
    assert_eq!(history.len(), 1);
    assert!(bob.core.history(10).check("main history").is_empty());
    bob.core.mark_chat_read(got.chat_id).check("read");
    assert_eq!(bob.core.chats().check("chats")[1].unread, 0);

    // An unknown chat is refused.
    let refused = bob.core.send_message_in(99, "nobody".into(), vec![], None);
    assert_eq!(
        refused.err().map(|e| e.kind),
        Some(CoreErrorKind::NoSuchChat)
    );
}

// spec: app-ui/read-mark
#[tokio::test(flavor = "multi_thread")]
async fn the_read_mark_is_the_newest_read_message() {
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let mut alice = device::member_of_group(&admin, "alice").await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    alice
        .wait_until("alice sees bob", |c| {
            c.members()
                .is_ok_and(|m| m.iter().any(|d| d.user == user("bob")))
        })
        .await;

    // Never read: the mark is 0.
    assert_eq!(bob.core.chat_read_mark(0).check("read mark"), 0);

    // Alice sends "one"; Bob receives it and reads up to it.
    let one = alice.core.send_text("one".into()).check("send one");
    wait_accepted(&mut alice, one).await;
    let one_at_bob = wait_text(&mut bob, "one").await;
    bob.core.mark_chat_read(0).check("read");
    assert_eq!(bob.core.chat_read_mark(0).check("read mark"), one_at_bob.id);
    assert_eq!(bob.core.chats().check("chats")[0].unread, 0);

    // Alice sends "two"; Bob receives it but does not read again. The mark
    // stays at "one" (the divider would pin here), and "two" is unread.
    let two = alice.core.send_text("two".into()).check("send two");
    wait_accepted(&mut alice, two).await;
    let two_at_bob = wait_text(&mut bob, "two").await;
    assert_eq!(bob.core.chat_read_mark(0).check("read mark"), one_at_bob.id);
    assert_eq!(bob.core.chats().check("chats")[0].unread, 1);

    // Reading again moves the mark to "two".
    bob.core.mark_chat_read(0).check("read");
    assert_eq!(bob.core.chat_read_mark(0).check("read mark"), two_at_bob.id);
}
