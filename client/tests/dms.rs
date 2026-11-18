//! DMs end to end against the real Pi (change `dms`): opening one, the
//! peer's devices added with no one tapping anything, messages that reach
//! only the DM, both users opening at once, several groups on one core,
//! closing on removal, and that the push stays content-free.

mod support;

use client::{
    Chat, ChatId, ChatKind, ClientError, Event, Message, MessageBody, MessageId, PushPlatform,
    PushToken, SendStatus,
};
use support::Check;
use support::device::{self, Device, member, user};
use support::pi::TestPi;

/// The admin, Alice and Bob, all in the group and seeing each other.
async fn group(pi: &TestPi) -> (Device, Device, Device) {
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

fn dm_with(device: &Device, peer: &str) -> Option<Chat> {
    device
        .client
        .chats()
        .check("chats")
        .into_iter()
        .find(|c| matches!(&c.kind, ChatKind::Dm { peer: p, .. } if *p == user(peer)))
}

/// Waits until `device` holds a DM with `peer` that `peer`'s devices are in.
async fn wait_dm_ready(device: &Device, peer: &str) -> ChatId {
    let wanted = user(peer);
    device
        .wait_until("the DM with its peer in it", |c| {
            c.chats().is_ok_and(|chats| {
                chats.iter().any(|chat| {
                    matches!(&chat.kind, ChatKind::Dm { peer, waiting: false, closed: false }
                        if *peer == wanted)
                })
            })
        })
        .await;
    dm_with(device, peer).check("the DM").id
}

// spec: direct-messages/opening-a-dm
// spec: direct-messages/dm-members-kept-complete
// spec: direct-messages/several-groups-in-the-core
#[tokio::test(flavor = "multi_thread")]
async fn alice_and_bob_talk_in_a_dm_and_the_admin_sees_nothing() {
    let pi = TestPi::start().await;
    let (mut admin, mut alice, mut bob) = group(&pi).await;
    assert!(alice.client.dms_available());

    let chat = alice.client.open_dm(&user("bob")).await.check("open DM");
    assert!(!chat.is_main());
    // Opening again finds the same DM.
    assert_eq!(
        alice.client.open_dm(&user("bob")).await.check("again"),
        chat
    );
    // Alice can write at once; it goes out once Bob's phone is in.
    let id = alice
        .client
        .send_message_in(chat, "hi bob, just you", &[], None)
        .check("send in the DM");
    wait_accepted(&mut alice, id).await;

    let got = wait_text(&mut bob, "hi bob, just you").await;
    let bobs_chat = wait_dm_ready(&bob, "alice").await;
    assert_eq!(got.chat, bobs_chat);
    assert!(!got.own_user);

    let id = bob
        .client
        .send_message_in(bobs_chat, "hi alice", &[], None)
        .check("reply in the DM");
    wait_accepted(&mut bob, id).await;
    let back = wait_text(&mut alice, "hi alice").await;
    assert_eq!(back.chat, chat);

    // The main group goes on as before, and holds none of it.
    let id = admin.client.send_text("group news").check("group send");
    wait_accepted(&mut admin, id).await;
    let news = wait_text(&mut bob, "group news").await;
    assert_eq!(news.chat, ChatId::MAIN);
    let main: Vec<MessageBody> = bob
        .client
        .history(50)
        .check("main history")
        .into_iter()
        .map(|m| m.body)
        .collect();
    assert_eq!(main, [MessageBody::Text("group news".into())]);
    let dm: Vec<MessageBody> = bob
        .client
        .history_in(bobs_chat, 50)
        .check("DM history")
        .into_iter()
        .map(|m| m.body)
        .collect();
    assert_eq!(
        dm,
        [
            MessageBody::Text("hi bob, just you".into()),
            MessageBody::Text("hi alice".into())
        ]
    );
    assert!(
        !admin
            .sees_within_quiet(|e| matches!(e, Event::Message(m) if m.chat != ChatId::MAIN))
            .await
    );
    assert!(dm_with(&admin, "alice").is_none() && dm_with(&admin, "bob").is_none());

    // The chat list: the group first, then the DM, with Alice's reply unread
    // until Bob reads it.
    let chats = bob.client.chats().check("chats");
    assert_eq!(chats[0].id, ChatId::MAIN);
    assert_eq!(chats[1].id, bobs_chat);
    assert_eq!(chats[1].unread, 1);
    bob.client.mark_chat_read(bobs_chat).check("read");
    assert_eq!(dm_with(&bob, "alice").check("DM").unread, 0);
}

// spec: direct-messages/opening-a-dm
#[tokio::test(flavor = "multi_thread")]
async fn both_open_at_once_and_end_up_in_one_dm() {
    let pi = TestPi::start().await;
    let (_admin, mut alice, mut bob) = group(&pi).await;
    let (to_bob, to_alice) = (user("bob"), user("alice"));
    let (a, b) = tokio::join!(alice.client.open_dm(&to_bob), bob.client.open_dm(&to_alice));
    let (a, b) = (a.check("Alice opens"), b.check("Bob opens"));
    let a = wait_dm_ready(&alice, "bob").await.max(a);
    let b = wait_dm_ready(&bob, "alice").await.max(b);
    let dms: i64 = pi
        .query(|db| {
            db.query_row(
                "SELECT count(*) FROM mls_groups WHERE kind = 'dm'",
                [],
                |r| r.get(0),
            )
        })
        .await;
    assert_eq!(dms, 1);
    let id = alice
        .client
        .send_message_in(a, "one DM", &[], None)
        .check("send");
    wait_accepted(&mut alice, id).await;
    assert_eq!(wait_text(&mut bob, "one DM").await.chat, b);
}

// spec: direct-messages/several-groups-in-the-core
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_keeps_both_groups() {
    let pi = TestPi::start().await;
    let (_admin, mut alice, mut bob) = group(&pi).await;
    let chat = alice.client.open_dm(&user("bob")).await.check("open");
    let id = alice
        .client
        .send_message_in(chat, "before", &[], None)
        .check("send");
    wait_accepted(&mut alice, id).await;
    let bobs_chat = wait_text(&mut bob, "before").await.chat;

    alice.restart().await;
    bob.restart().await;
    let id = alice
        .client
        .send_message_in(chat, "after", &[], None)
        .check("send after restart");
    wait_accepted(&mut alice, id).await;
    assert_eq!(wait_text(&mut bob, "after").await.chat, bobs_chat);
    let id = bob.client.send_text("in the group").check("group send");
    wait_accepted(&mut bob, id).await;
    assert_eq!(
        wait_text(&mut alice, "in the group").await.chat,
        ChatId::MAIN
    );
}

// spec: direct-messages/dm-closed-on-user-removal
#[tokio::test(flavor = "multi_thread")]
async fn removing_bob_closes_alices_dm_with_him() {
    let pi = TestPi::start().await;
    let (admin, mut alice, mut bob) = group(&pi).await;
    let chat = alice.client.open_dm(&user("bob")).await.check("open");
    let id = alice
        .client
        .send_message_in(chat, "still here?", &[], None)
        .check("send");
    wait_accepted(&mut alice, id).await;
    wait_text(&mut bob, "still here?").await;

    admin.client.remove_user(&user("bob")).await.check("remove");
    alice
        .wait_until("the DM closed", |c| {
            c.chats().is_ok_and(|chats| {
                chats
                    .iter()
                    .any(|chat| matches!(chat.kind, ChatKind::Dm { closed: true, .. }))
            })
        })
        .await;
    // History stays; nothing more goes in.
    assert_eq!(alice.client.history_in(chat, 10).check("history").len(), 1);
    assert!(matches!(
        alice.client.send_message_in(chat, "hello?", &[], None),
        Err(ClientError::ChatClosed)
    ));
    // Closed DMs come last.
    let chats = alice.client.chats().check("chats");
    assert_eq!(chats.last().map(|c| c.id), Some(chat));
}

// spec: direct-messages/dm-content-stays-encrypted
#[tokio::test(flavor = "multi_thread")]
async fn a_dm_push_is_the_same_content_free_ping() {
    let pi = TestPi::start().await;
    let (_admin, mut alice, mut bob) = group(&pi).await;
    let chat = alice.client.open_dm(&user("bob")).await.check("open");
    let id = alice
        .client
        .send_message_in(chat, "warm up", &[], None)
        .check("send");
    wait_accepted(&mut alice, id).await;
    wait_text(&mut bob, "warm up").await;

    bob.client
        .set_push_token(Some(PushToken {
            platform: PushPlatform::Fcm,
            token: "bob-token".into(),
        }))
        .await
        .check("push token");
    let bob_device = bob.device_id();
    bob.client.stop();
    pi.wait_disconnected(&bob_device).await;

    let id = alice
        .client
        .send_message_in(chat, "secret DM words", &[], None)
        .check("send");
    wait_accepted(&mut alice, id).await;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    while pi.pushes.count(&bob_device) == 0 {
        assert!(tokio::time::Instant::now() < deadline, "no wake ping");
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let pings = pi.pushes.sent();
    assert!(pings.iter().all(|(device, token)| {
        device.as_slice() == bob_device.as_slice() && token == "bob-token"
    }));
    let stored: Vec<Vec<u8>> = pi
        .query(|db| {
            let mut stmt = db.prepare("SELECT ciphertext FROM envelopes")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect()
        })
        .await;
    assert!(
        stored
            .iter()
            .all(|c| !support::contains(c, b"secret DM words"))
    );
}

// spec: direct-messages/dm-members-kept-complete
#[tokio::test(flavor = "multi_thread")]
async fn bobs_new_tablet_joins_the_dm_with_no_one_tapping() {
    let pi = TestPi::start().await;
    let (_admin, mut alice, mut bob) = group(&pi).await;
    let chat = alice.client.open_dm(&user("bob")).await.check("open");
    let id = alice
        .client
        .send_message_in(chat, "before the tablet", &[], None)
        .check("send");
    wait_accepted(&mut alice, id).await;
    wait_text(&mut bob, "before the tablet").await;

    // Bob links a tablet; his phone adds it to the group, then a member of
    // the DM adds it there.
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
    let tablets_chat = wait_dm_ready(&tablet, "alice").await;

    let id = alice
        .client
        .send_message_in(chat, "hello tablet", &[], None)
        .check("send");
    wait_accepted(&mut alice, id).await;
    assert_eq!(
        wait_text(&mut tablet, "hello tablet").await.chat,
        tablets_chat
    );
    // History from before it joined is not there (no history for new
    // devices).
    let history = tablet.client.history_in(tablets_chat, 10).check("history");
    assert_eq!(history.len(), 1);
}

// spec: direct-messages/inactivity-across-groups
#[tokio::test(flavor = "multi_thread")]
async fn bob_back_from_holiday_is_re_added_to_the_dm_too() {
    let pi = TestPi::start().await;
    let (mut admin, mut alice, mut bob) = group(&pi).await;
    let chat = alice.client.open_dm(&user("bob")).await.check("open");
    let id = alice
        .client
        .send_message_in(chat, "see you", &[], None)
        .check("send");
    wait_accepted(&mut alice, id).await;
    let bobs_chat = wait_text(&mut bob, "see you").await.chat;

    bob.client.stop();
    pi.wait_offline_recorded(&bob.device_id()).await;
    pi.clock
        .advance(std::time::Duration::from_secs(10 * 24 * 60 * 60));
    for device in [&mut admin, &mut alice] {
        device
            .wait_event("the stream to be closed", |e| {
                matches!(e, Event::Connection(client::ConnectionState::Connecting))
            })
            .await;
        device.wait_connected().await;
    }
    pi.sweep().await;
    // The admin commits the Pi's Remove in the group, Alice in the DM.
    admin.wait_members(&[member(&admin), member(&alice)]).await;
    alice
        .wait_until("Bob's phone out of the DM", |c| {
            c.chats().is_ok_and(|chats| {
                chats
                    .iter()
                    .any(|chat| matches!(chat.kind, ChatKind::Dm { waiting: true, .. }))
            })
        })
        .await;

    // Bob's phone is back: re-added to the group, then to the DM.
    bob.client.start().await.check("start bob");
    bob.wait_event("rejoined", |e| matches!(e, Event::Rejoined))
        .await;
    wait_dm_ready(&alice, "bob").await;
    let id = alice
        .client
        .send_message_in(chat, "welcome back", &[], None)
        .check("send");
    wait_accepted(&mut alice, id).await;
    assert_eq!(wait_text(&mut bob, "welcome back").await.chat, bobs_chat);
}
