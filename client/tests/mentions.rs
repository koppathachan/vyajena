//! Mentions and replies end to end against the real Pi (change `mentions`):
//! who is mentioned (message-content: Mentions), replies and their lookup
//! (message-content: Replies; client-runtime: Reply lookup), unseen
//! mentions across restarts (client-runtime: Unseen mentions), and that the
//! Pi learns nothing new (message-content: Mentions and replies stay
//! encrypted).

mod support;

use client::{
    ClientError, Event, Mention, MentionTarget, Message, MessageBody, MessageId, PushPlatform,
    PushToken, SendStatus,
};
use support::Check;
use support::device::{self, Device, member, user};
use support::pi::TestPi;

/// The admin, Alice and Bob, all in the group and seeing each other.
async fn chat(pi: &TestPi) -> (Device, Device, Device) {
    let admin = device::admin_with_group(pi).await;
    let alice = device::member_of_group(&admin, "alice").await;
    let bob = device::member_of_group(&admin, "bob").await;
    let everyone = [member(&admin), member(&alice), member(&bob)];
    for device in [&admin, &alice, &bob] {
        device.wait_members(&everyone).await;
    }
    (admin, alice, bob)
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

fn of(target: MentionTarget, start: usize, len: usize) -> Mention {
    Mention { target, start, len }
}

// spec: message-content/mentions
// spec: client-runtime/unseen-mentions
#[tokio::test(flavor = "multi_thread")]
async fn alice_mentions_bob_and_only_bob_is_mentioned() {
    let pi = TestPi::start().await;
    let (mut admin, mut alice, mut bob) = chat(&pi).await;

    let mentions = [of(MentionTarget::User(user("bob")), 3, 4)];
    let id = alice
        .client
        .send_message("hi @Bob", &mentions, None)
        .check("send");
    wait_accepted(&mut alice, id).await;

    let at_bob = wait_text(&mut bob, "hi @Bob").await;
    assert_eq!(at_bob.mentions, mentions);
    assert!(at_bob.mentions_me);
    assert_eq!(bob.client.unseen_mentions().check("unseen"), [at_bob.id]);

    let at_admin = wait_text(&mut admin, "hi @Bob").await;
    assert_eq!(at_admin.mentions, mentions);
    assert!(!at_admin.mentions_me);
    assert!(admin.client.unseen_mentions().check("unseen").is_empty());

    // Alice's own copy keeps the mention but never mentions her.
    let own = alice.client.message(id).check("own").check("own");
    assert_eq!(own.mentions, mentions);
    assert!(!own.mentions_me);
    assert!(alice.client.unseen_mentions().check("unseen").is_empty());

    // A bad range is refused before anything is stored.
    let before = alice.client.history(100).check("history").len();
    let bad = alice
        .client
        .send_message("hi", &[of(MentionTarget::User(user("bob")), 1, 8)], None);
    assert!(matches!(bad, Err(ClientError::InvalidMention)), "{bad:?}");
    assert_eq!(alice.client.history(100).check("history").len(), before);
}

// spec: message-content/mentions
#[tokio::test(flavor = "multi_thread")]
async fn bob_who_is_not_the_admin_mentions_everyone() {
    let pi = TestPi::start().await;
    let (mut admin, mut alice, mut bob) = chat(&pi).await;

    let mentions = [of(MentionTarget::Everyone, 0, 9)];
    let id = bob
        .client
        .send_message("@everyone dinner", &mentions, None)
        .check("send");
    wait_accepted(&mut bob, id).await;
    for device in [&mut admin, &mut alice] {
        let message = wait_text(device, "@everyone dinner").await;
        assert!(message.mentions_me, "{}", device.user());
        assert_eq!(
            device.client.unseen_mentions().check("unseen"),
            [message.id]
        );
    }
    assert!(bob.client.unseen_mentions().check("unseen").is_empty());
    assert!(!bob.client.message(id).check("own").check("own").mentions_me);
}

// spec: message-content/replies
// spec: client-runtime/reply-lookup
#[tokio::test(flavor = "multi_thread")]
async fn bob_replies_to_alice_and_everyone_finds_the_original() {
    let pi = TestPi::start().await;
    let (mut admin, mut alice, mut bob) = chat(&pi).await;

    let original = alice.client.send_text("dinner at 8?").check("send");
    wait_accepted(&mut alice, original).await;
    let at_bob = wait_text(&mut bob, "dinner at 8?").await;
    let at_admin = wait_text(&mut admin, "dinner at 8?").await;

    let reply = bob
        .client
        .send_message("yes", &[], Some(at_bob.id))
        .check("reply");
    wait_accepted(&mut bob, reply).await;

    // Bob's own copy, Alice's and the admin's each point at their own local
    // id of the original.
    let own = bob.client.message(reply).check("own").check("own");
    let own_reply = own.reply.check("own reply");
    assert_eq!(own_reply.to, user("alice"));
    assert_eq!(own_reply.quote, "dinner at 8?");
    assert_eq!(own_reply.original, Some(at_bob.id));
    for (device, original_here) in [(&mut alice, original), (&mut admin, at_admin.id)] {
        let message = wait_text(device, "yes").await;
        let quoted = message.reply.check("reply");
        assert_eq!(quoted.to, user("alice"));
        assert_eq!(quoted.quote, "dinner at 8?");
        assert_eq!(quoted.original, Some(original_here));
    }

    // A long original is quoted cut to 100 characters.
    let long = format!("{} end", "word ".repeat(40));
    let long_id = alice.client.send_text(&long).check("send");
    wait_accepted(&mut alice, long_id).await;
    let long_at_bob = wait_text(&mut bob, &long).await;
    let r = bob
        .client
        .send_message("ok", &[], Some(long_at_bob.id))
        .check("reply");
    let quote = bob
        .client
        .message(r)
        .check("own")
        .check("own")
        .reply
        .check("reply")
        .quote;
    assert_eq!(quote.chars().count(), 100);
    assert!(quote.ends_with('…'));

    // Replying to something that is not a stored chat message stores
    // nothing.
    let before = bob.client.history(100).check("history").len();
    let bad = bob.client.send_message("?", &[], Some(MessageId(999_999)));
    assert!(matches!(bad, Err(ClientError::NoSuchMessage)), "{bad:?}");
    assert_eq!(bob.client.history(100).check("history").len(), before);
}

// spec: message-content/replies
// spec: client-runtime/reply-lookup
#[tokio::test(flavor = "multi_thread")]
async fn carol_who_joined_later_sees_the_quote_without_the_original() {
    let pi = TestPi::start().await;
    let (admin, mut alice, mut bob) = chat(&pi).await;

    let original = alice.client.send_text("dinner at 8?").check("send");
    wait_accepted(&mut alice, original).await;
    let at_bob = wait_text(&mut bob, "dinner at 8?").await;

    let mut carol = device::member_of_group(&admin, "carol").await;
    carol
        .wait_members(&[member(&admin), member(&alice), member(&bob), member(&carol)])
        .await;
    bob.wait_members(&[member(&admin), member(&alice), member(&bob), member(&carol)])
        .await;
    let reply = bob
        .client
        .send_message("yes", &[], Some(at_bob.id))
        .check("reply");
    wait_accepted(&mut bob, reply).await;

    let message = wait_text(&mut carol, "yes").await;
    let quoted = message.reply.check("reply");
    assert_eq!(quoted.to, user("alice"));
    assert_eq!(quoted.quote, "dinner at 8?");
    assert_eq!(quoted.original, None);
}

// spec: client-runtime/unseen-mentions
#[tokio::test(flavor = "multi_thread")]
async fn an_unseen_mention_survives_a_restart_until_marked_seen() {
    let pi = TestPi::start().await;
    let (_admin, mut alice, mut bob) = chat(&pi).await;

    let first = [of(MentionTarget::User(user("bob")), 0, 4)];
    let a = alice
        .client
        .send_message("@Bob look", &first, None)
        .check("send");
    wait_accepted(&mut alice, a).await;
    let b = alice
        .client
        .send_message("@Bob and this", &first, None)
        .check("send");
    wait_accepted(&mut alice, b).await;
    let one = wait_text(&mut bob, "@Bob look").await;
    let two = wait_text(&mut bob, "@Bob and this").await;

    bob.restart().await;
    assert_eq!(
        bob.client.unseen_mentions().check("unseen"),
        [one.id, two.id]
    );

    assert!(bob.client.mark_mention_seen(one.id).check("mark"));
    assert!(!bob.client.mark_mention_seen(one.id).check("mark again"));
    bob.restart().await;
    assert_eq!(bob.client.unseen_mentions().check("unseen"), [two.id]);
}

// spec: message-content/mentions-and-replies-stay-encrypted
#[tokio::test(flavor = "multi_thread")]
async fn a_mention_of_offline_bob_wakes_him_like_any_message() {
    let pi = TestPi::start().await;
    let (mut admin, mut alice, bob) = chat(&pi).await;

    bob.client
        .set_push_token(Some(PushToken {
            platform: PushPlatform::Fcm,
            token: "bob-token".into(),
        }))
        .await
        .check("token");
    bob.client.stop();
    pi.wait_offline_recorded(&bob.device_id()).await;

    let original = admin.client.send_text("secret plans").check("send");
    wait_accepted(&mut admin, original).await;
    let at_alice = wait_text(&mut alice, "secret plans").await;
    let id = alice
        .client
        .send_message(
            "@Bob @everyone agreed",
            &[
                of(MentionTarget::User(user("bob")), 0, 4),
                of(MentionTarget::Everyone, 5, 9),
            ],
            Some(at_alice.id),
        )
        .check("send");
    wait_accepted(&mut alice, id).await;

    // One ping to Bob's phone, carrying only its id and token: the provider
    // interface has nothing else to give, and the providers' text is the
    // fixed "New message" (server push tests).
    let bob_id = bob.device_id();
    let deadline = tokio::time::Instant::now() + support::pi::WAIT;
    while pi.pushes.count(&bob_id) == 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "no wake ping for Bob"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(pi.pushes.sent(), [(bob_id, "bob-token".to_owned())]);

    // Neither the text, the quote nor the mention reached the Pi's store.
    let rows: Vec<Vec<u8>> = pi
        .query(|db| {
            let mut statement = db.prepare("SELECT ciphertext FROM envelopes WHERE kind = 1")?;
            let rows = statement.query_map([], |row| row.get(0))?;
            rows.collect()
        })
        .await;
    assert!(!rows.is_empty(), "the Pi holds the message");
    for row in rows {
        for secret in [&b"agreed"[..], b"secret plans", b"everyone"] {
            assert!(!support::contains(&row, secret));
        }
    }
}
