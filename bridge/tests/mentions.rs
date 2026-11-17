//! Mentions and replies through the bridge (change `mentions`): Dart's
//! UTF-16 ranges reach the core and come back as the same ranges, replies
//! carry their quote and original, and unseen mentions are listed and
//! cleared.

mod support;

use bridge::api::error::CoreErrorKind;
use bridge::api::types::{ChatBody, ChatMessage, CoreEvent, MentionSpan, MentionTarget, SendState};
use support::Check;
use support::device::{self, Device, user};
use support::pi::TestPi;

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

// spec: app-ui/mentions-of-me
// spec: app-ui/reply-to-a-message
#[tokio::test(flavor = "multi_thread")]
async fn alice_mentions_bob_after_an_emoji_and_bob_replies() {
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

    // "👋 @Bobby": the mention starts at UTF-16 unit 3 (the emoji is 2).
    let text = "👋 @Bobby hi";
    let at_bob = MentionSpan {
        target: MentionTarget::User { user: user("bob") },
        start: 3,
        length: 6,
    };
    let everyone = MentionSpan {
        target: MentionTarget::Everyone,
        start: 0,
        length: 1,
    };
    // A span that splits the emoji is refused.
    let bad = alice
        .core
        .send_message(text.to_owned(), vec![everyone], None);
    assert_eq!(
        bad.err().map(|e| e.kind),
        Some(CoreErrorKind::InvalidMention)
    );

    let id = alice
        .core
        .send_message(text.to_owned(), vec![at_bob.clone()], None)
        .check("send");
    wait_accepted(&mut alice, id).await;
    let received = wait_text(&mut bob, text).await;
    assert_eq!(received.mentions, std::slice::from_ref(&at_bob));
    assert!(received.mentions_me);
    assert_eq!(bob.core.unseen_mentions().check("unseen"), [received.id]);
    assert!(bob.core.mark_mention_seen(received.id).check("seen"));
    assert!(bob.core.unseen_mentions().check("unseen").is_empty());

    let reply = bob
        .core
        .send_message("yes".to_owned(), vec![], Some(received.id))
        .check("reply");
    wait_accepted(&mut bob, reply).await;
    let at_alice = wait_text(&mut alice, "yes").await;
    let quote = at_alice.reply.check("reply");
    assert_eq!(quote.to, user("alice"));
    assert_eq!(quote.quote, text);
    assert_eq!(quote.original_id, Some(id));

    let missing = bob.core.send_message("?".to_owned(), vec![], Some(424_242));
    assert_eq!(
        missing.err().map(|e| e.kind),
        Some(CoreErrorKind::NoSuchMessage)
    );
}
