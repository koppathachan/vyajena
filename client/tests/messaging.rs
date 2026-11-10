//! Messaging on the device, end to end against the real Pi (client-core
//! tasks 6.1–6.3, 5.7): content bodies (message-content), send status
//! (client-runtime: Send status), the local store around crashes
//! (client-runtime: Local message store), and one ordered event stream
//! (client-runtime: Events in order).

mod support;

use std::time::Duration;

use client::messaging::{self, Decoded, MAX_CONTENT_BYTES};
use client::store::{OutgoingMessage, Store};
use client::{ClientError, Clock as _, ConnectionState, Event, MessageBody, MessageId, SendStatus};
use support::Check;
use support::device::{self, Device, member, user};
use support::pi::{ADMIN, TestPi};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

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

fn is_text(event: &Event, text: &str) -> bool {
    matches!(event, Event::Message(m) if m.body == MessageBody::Text(text.to_owned()))
}

async fn wait_text(device: &mut Device, text: &str) -> client::Message {
    match device
        .wait_event(&format!("the message {text:?}"), |e| is_text(e, text))
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

fn texts(device: &Device) -> Vec<String> {
    device
        .client
        .history(1000)
        .check("history")
        .into_iter()
        .filter_map(|m| match m.body {
            MessageBody::Text(text) => Some(text),
            _ => None,
        })
        .collect()
}

/// A second handle on `device`'s database, to store an own message with a
/// body the public API does not send (as another app version would).
fn raw_store(device: &Device) -> Store {
    Store::open(&device.data_dir().join(client::DATABASE_FILE), &device.keys)
        .check("open the device's store")
}

fn store_raw_outgoing(device: &Device, content: &[u8]) {
    let mut msg_id = [0u8; 16];
    getrandom::fill(&mut msg_id).check("randomness");
    let sender = device.user().to_string();
    raw_store(device)
        .insert_outgoing(&OutgoingMessage {
            msg_id,
            sender_user: &sender,
            sender_device: device.device_id(),
            content,
            created_at: device.clock.now(),
        })
        .check("store a raw message");
}

/// Stops `device` and waits until the Pi has seen its stream close.
async fn go_offline(pi: &TestPi, device: &Device) {
    device.client.stop();
    let id = device.device_id();
    let deadline = tokio::time::Instant::now() + device::WAIT;
    while pi.is_connected(&id) {
        assert!(tokio::time::Instant::now() < deadline, "still connected");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
}

// spec: message-content/inner-content-format
// spec: client-runtime/send-status
// spec: client-runtime/events-in-order
#[tokio::test(flavor = "multi_thread")]
async fn text_messages_reach_everyone_in_order() {
    let pi = TestPi::start().await;
    let (mut admin, mut alice, mut bob) = chat(&pi).await;

    let ids: Vec<MessageId> = ["one", "two", "three"]
        .iter()
        .map(|text| alice.client.send_text(text).check("send"))
        .collect();
    for id in &ids {
        wait_accepted(&mut alice, *id).await;
    }
    for device in [&mut bob, &mut admin] {
        for text in ["one", "two", "three"] {
            let message = wait_text(device, text).await;
            assert_eq!(message.sender, user("alice"));
            assert_eq!(message.sender_device, alice.device_id());
            assert!(!message.outgoing);
            assert_eq!(message.sent_at, Some(alice.clock.now()));
            assert!(message.accepted_at.is_some());
        }
        assert_eq!(texts(device), ["one", "two", "three"]);
    }

    // Alice's own history: her plaintext, accepted, in the same order.
    let own = alice.client.history(10).check("history");
    assert_eq!(texts(&alice), ["one", "two", "three"]);
    assert!(
        own.iter()
            .all(|m| m.outgoing && m.status == Some(SendStatus::Accepted))
    );
    assert_eq!(
        alice
            .client
            .message(ids[0])
            .check("message")
            .map(|m| m.body),
        Some(MessageBody::Text("one".into()))
    );

    // Bob answers; everyone's history interleaves in server_seq order.
    let id = bob.client.send_text("hi alice").check("send");
    wait_accepted(&mut bob, id).await;
    wait_text(&mut alice, "hi alice").await;
    wait_text(&mut admin, "hi alice").await;
    for device in [&alice, &bob, &admin] {
        assert_eq!(texts(device), ["one", "two", "three", "hi alice"]);
    }
}

// spec: message-content/text-only
#[tokio::test(flavor = "multi_thread")]
async fn a_very_long_paste_is_refused_and_the_longest_allowed_text_is_accepted() {
    let pi = TestPi::start().await;
    let (_admin, mut alice, mut bob) = chat(&pi).await;

    let paste = "x".repeat(protocol::wire_transport::MAX_APPLICATION_CIPHERTEXT_BYTES);
    let refused = alice.client.send_text(&paste);
    assert!(matches!(refused, Err(ClientError::TooLong)), "{refused:?}");
    assert!(texts(&alice).is_empty(), "nothing is stored");

    // The longest text that fits (UTF-8, with multi-byte characters)
    // encrypts within the cap: the Pi accepts it.
    let mut longest = "é".repeat(MAX_CONTENT_BYTES / 2);
    while messaging::encode_text(&longest, alice.clock.now()).is_err() {
        longest.pop();
    }
    assert!(longest.len() > MAX_CONTENT_BYTES - 64);
    let id = alice.client.send_text(&longest).check("longest");
    wait_accepted(&mut alice, id).await;
    let received = wait_text(&mut bob, &longest).await;
    assert_eq!(received.sender, user("alice"));
}

// spec: message-content/inner-content-format
#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_content_body_decodes_as_unset_and_shows_a_placeholder() {
    // A Content with a send time and a body field this version does not know
    // (field 9): the oneof decodes as unset.
    let unknown: &[u8] = &[0x0A, 0x02, 0x08, 0x01, 0x4A, 0x01, b'x'];
    let decoded = messaging::decode(unknown);
    assert_eq!(decoded.body, Decoded::Unsupported);
    assert!(decoded.sent_at.is_some());
    assert_eq!(messaging::decode(b"\xff\xff").body, Decoded::Unsupported);

    // End to end: a newer app's message reaches Bob as a placeholder, and
    // Bob goes on reading what follows.
    let pi = TestPi::start().await;
    let (_admin, mut alice, mut bob) = chat(&pi).await;
    store_raw_outgoing(&alice, unknown);
    let placeholder = bob
        .wait_event(
            "the placeholder",
            |e| matches!(e, Event::Message(m) if m.body == MessageBody::Unsupported),
        )
        .await;
    let Event::Message(placeholder) = placeholder else {
        unreachable!()
    };
    assert_eq!(placeholder.sender, user("alice"));
    let id = alice.client.send_text("after").check("send");
    wait_accepted(&mut alice, id).await;
    wait_text(&mut bob, "after").await;
    let history = bob.client.history(10).check("history");
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].body, MessageBody::Unsupported);
}

// spec: message-content/group-settings-travel-encrypted
#[tokio::test(flavor = "multi_thread")]
async fn only_admin_devices_change_the_group_settings() {
    let pi = TestPi::start().await;
    let (mut admin, mut alice, mut bob) = chat(&pi).await;

    let id = admin
        .client
        .set_group_settings(Some("Family"))
        .check("rename");
    wait_accepted(&mut admin, id).await;
    for device in [&mut admin, &mut alice, &mut bob] {
        device
            .wait_event("the new name", |e| {
                matches!(e, Event::GroupSettingsChanged { name } if name.as_deref() == Some("Family"))
            })
            .await;
        assert_eq!(
            device
                .client
                .group_settings()
                .check("settings")
                .name
                .as_deref(),
            Some("Family")
        );
    }
    assert!(texts(&alice).is_empty(), "settings are not chat messages");

    // A non-admin's app refuses; and a GroupSettings a non-admin device
    // sends anyway is ignored by everyone.
    let refused = bob.client.set_group_settings(Some("Bob's club"));
    assert!(matches!(refused, Err(ClientError::NotAdmin)), "{refused:?}");
    let forged =
        messaging::encode_group_settings(Some("Bob's club"), bob.clock.now()).check("encode");
    store_raw_outgoing(&bob, &forged);
    let id = bob.client.send_text("after the rename").check("send");
    wait_accepted(&mut bob, id).await;
    for device in [&mut alice, &mut admin] {
        wait_text(device, "after the rename").await;
        assert!(
            !device
                .events()
                .iter()
                .any(|e| matches!(e, Event::GroupSettingsChanged { name } if name.as_deref() == Some("Bob's club")))
        );
        assert_eq!(
            device
                .client
                .group_settings()
                .check("settings")
                .name
                .as_deref(),
            Some("Family")
        );
    }
}

// spec: message-content/user-owned-profiles
#[tokio::test(flavor = "multi_thread")]
async fn profiles_apply_to_their_sender_and_always_show_the_username() {
    let pi = TestPi::start().await;
    let (mut admin, mut alice, mut bob) = chat(&pi).await;
    assert_eq!(
        bob.client.display_name(&user("alice")).check("name"),
        "alice",
        "the username when no display name is set"
    );

    let id = alice
        .client
        .set_profile(Some("Ali"), Some("climbing, tea"))
        .check("profile");
    wait_accepted(&mut alice, id).await;
    for device in [&mut bob, &mut admin, &mut alice] {
        device
            .wait_event("Alice's profile", |e| {
                matches!(e, Event::ProfileChanged { user: u, display_name, about }
                    if *u == user("alice")
                        && display_name.as_deref() == Some("Ali")
                        && about.as_deref() == Some("climbing, tea"))
            })
            .await;
        assert_eq!(
            device.client.display_name(&user("alice")).check("name"),
            "Ali (alice)"
        );
    }
    let profile = bob.client.profile(&user("alice")).check("profile");
    assert_eq!(profile.about.as_deref(), Some("climbing, tea"));

    // Bob calls himself "akhil": he still reads as bob, and the admin's
    // own profile is untouched.
    let id = bob.client.set_profile(Some("akhil"), None).check("profile");
    wait_accepted(&mut bob, id).await;
    alice
        .wait_event(
            "Bob's profile",
            |e| matches!(e, Event::ProfileChanged { user: u, .. } if *u == user("bob")),
        )
        .await;
    assert_eq!(
        alice.client.display_name(&user("bob")).check("name"),
        "akhil (bob)"
    );
    assert_eq!(alice.client.display_name(&user(ADMIN)).check("name"), ADMIN);
    assert!(texts(&alice).is_empty(), "profiles are not chat messages");
}

// spec: client-runtime/send-status
// spec: message-delivery/strict-epoch-rule
#[tokio::test(flavor = "multi_thread")]
async fn a_message_sent_across_a_commit_is_resent_in_the_new_epoch() {
    let pi = TestPi::start().await;
    let (admin, mut alice, _bob) = chat(&pi).await;
    let epoch = alice.epoch().check("epoch");

    // Alice's laptop is away while the admin adds Carol; she writes "hi"
    // meanwhile. On reconnect the core sends it before it read the commit,
    // so the Pi answers STALE_EPOCH.
    go_offline(&pi, &alice).await;
    let mut carol = device::member_of_group(&admin, "carol").await;
    let id = alice.client.send_text("hi").check("send while away");
    assert_eq!(
        alice
            .client
            .message(id)
            .check("message")
            .and_then(|m| m.status),
        Some(SendStatus::Sending)
    );
    alice.client.start().await.check("start");

    // The core applies the commit, encrypts again in the new epoch, and the
    // message goes from sending to accepted, never failed.
    wait_accepted(&mut alice, id).await;
    assert_eq!(alice.epoch(), Some(epoch + 1));
    assert!(!alice.events().iter().any(|e| matches!(
        e,
        Event::SendStatus {
            status: SendStatus::Failed { .. },
            ..
        }
    )));
    // Carol, who has only the new epoch's keys, reads it.
    let message = wait_text(&mut carol, "hi").await;
    assert_eq!(message.sender, user("alice"));
}

// spec: client-runtime/local-message-store
// spec: message-delivery/idempotent-sends
#[tokio::test(flavor = "multi_thread")]
async fn an_own_message_survives_a_crash_before_accepted() {
    let pi = TestPi::start().await;
    let (_admin, mut alice, mut bob) = chat(&pi).await;

    pi.relay.cut();
    alice
        .wait_event("the drop", |e| {
            matches!(e, Event::Connection(ConnectionState::Connecting))
        })
        .await;
    let id = alice.client.send_text("hi").check("send");
    // The laptop crashes before Accepted arrives.
    alice.restart().await;
    let history = alice.client.history(10).check("history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].id, id);
    assert_eq!(history[0].body, MessageBody::Text("hi".into()));
    assert_eq!(history[0].status, Some(SendStatus::Sending));

    // Back online, the core resends it with the same msg_id.
    pi.relay.restore();
    wait_accepted(&mut alice, id).await;
    wait_text(&mut bob, "hi").await;
    assert!(!bob.sees_within_quiet(|e| is_text(e, "hi")).await);
    assert_eq!(texts(&bob), ["hi"]);
    let alice_id = alice.device_id();
    let stored: i64 = pi
        .query(move |db| {
            db.query_row(
                "SELECT count(*) FROM dedup WHERE sender_device_id = ?1",
                [alice_id.as_slice()],
                |row| row.get(0),
            )
        })
        .await;
    assert_eq!(stored, 1, "the Pi accepted it once");
}

// spec: client-runtime/local-message-store
// spec: message-delivery/acknowledgement
#[tokio::test(flavor = "multi_thread")]
async fn a_message_redelivered_after_a_crash_before_the_ack_is_seen_once() {
    let pi = TestPi::start().await;
    let (_admin, mut alice, mut bob) = chat(&pi).await;
    go_offline(&pi, &bob).await;
    let id = alice.client.send_text("hi").check("send");
    wait_accepted(&mut alice, id).await;

    // Keep what the Pi holds for Bob, so it can be put back as if his ack
    // never arrived.
    let bob_id = bob.device_id();
    let held: i64 = pi
        .query(move |db| {
            db.execute(
                "CREATE TABLE saved_deliveries AS SELECT * FROM deliveries WHERE device_id = ?1",
                [bob_id.as_slice()],
            )?;
            db.execute_batch("CREATE TABLE saved_envelopes AS SELECT * FROM envelopes")?;
            db.query_row(
                "SELECT max(server_seq) FROM saved_deliveries WHERE device_id = ?1",
                [bob_id.as_slice()],
                |row| row.get(0),
            )
        })
        .await;

    // Bob stores it and acks it.
    bob.client.start().await.check("start bob");
    wait_text(&mut bob, "hi").await;
    let deadline = tokio::time::Instant::now() + device::WAIT;
    loop {
        let acked: i64 = pi
            .query(move |db| {
                db.query_row(
                    "SELECT acked_seq FROM cursors WHERE device_id = ?1",
                    [bob_id.as_slice()],
                    |row| row.get(0),
                )
            })
            .await;
        if acked >= held {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "Bob never acked");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // The ack is lost in a crash: the Pi delivers it again after restart.
    go_offline(&pi, &bob).await;
    pi.query(move |db| {
        db.execute_batch(
            "INSERT OR IGNORE INTO envelopes SELECT * FROM saved_envelopes;
             INSERT OR IGNORE INTO deliveries SELECT * FROM saved_deliveries;",
        )?;
        db.execute(
            "UPDATE cursors SET acked_seq = ?2 - 1 WHERE device_id = ?1",
            rusqlite::params![bob_id.as_slice(), held],
        )
    })
    .await;
    bob.restart().await;
    bob.wait_connected().await;
    assert!(!bob.sees_within_quiet(|e| is_text(e, "hi")).await);
    assert_eq!(texts(&bob), ["hi"]);
    let acked: i64 = pi
        .query(move |db| {
            db.query_row(
                "SELECT acked_seq FROM cursors WHERE device_id = ?1",
                [bob_id.as_slice()],
                |row| row.get(0),
            )
        })
        .await;
    assert_eq!(acked, held, "the redelivery was acked again");
}

// spec: client-runtime/connection-lifecycle
// spec: client-runtime/events-in-order
#[tokio::test(flavor = "multi_thread")]
async fn after_a_network_drop_messages_arrive_in_order() {
    let pi = TestPi::start().await;
    let (mut admin, _alice, mut bob) = chat(&pi).await;

    pi.relay.cut();
    bob.wait_event("the drop", |e| {
        matches!(e, Event::Connection(ConnectionState::Connecting))
    })
    .await;
    let ids: Vec<MessageId> = (1..=5)
        .map(|n| admin.client.send_text(&format!("m{n}")).check("send"))
        .collect();
    tokio::time::sleep(Duration::from_millis(300)).await;
    pi.relay.restore();
    for id in ids {
        wait_accepted(&mut admin, id).await;
    }
    for n in 1..=5 {
        wait_text(&mut bob, &format!("m{n}")).await;
    }
    assert_eq!(texts(&bob), ["m1", "m2", "m3", "m4", "m5"]);
}

// spec: client-runtime/events-in-order
// spec: group-membership/automatic-re-add
#[tokio::test(flavor = "multi_thread")]
async fn back_from_holiday_the_ui_hears_of_the_removal_then_newer_messages() {
    let pi = TestPi::start().await;
    let (mut admin, _alice, mut bob) = chat(&pi).await;
    go_offline(&pi, &bob).await;

    // Ten days away: the Pi marks Bob's laptop inactive and the admin
    // commits its Remove.
    pi.clock.advance(10 * DAY);
    admin
        .wait_event("the stream to be closed", |e| {
            matches!(e, Event::Connection(ConnectionState::Connecting))
        })
        .await;
    admin.wait_connected().await;
    pi.sweep().await;
    admin
        .wait_until("Bob's laptop removed", |c| {
            c.members()
                .is_ok_and(|m| !m.iter().any(|m| m.user == user("bob")))
        })
        .await;

    bob.client.start().await.check("start bob");
    bob.wait_event("rejoined", |e| matches!(e, Event::Rejoined))
        .await;
    bob.wait_until("in the admin's epoch", |c| {
        c.epoch().ok().flatten() == admin.epoch()
    })
    .await;
    let id = admin.client.send_text("welcome back").check("send");
    wait_accepted(&mut admin, id).await;
    wait_text(&mut bob, "welcome back").await;

    // One stream, in order: removed (with when messages stop), rejoined,
    // then the message from after the re-add.
    let events = bob.events();
    let position = |f: &dyn Fn(&Event) -> bool| events.iter().position(f).check("event");
    let removed = position(&|e| matches!(e, Event::RemovedForInactivity { .. }));
    let rejoined = position(&|e| matches!(e, Event::Rejoined));
    let message = position(&|e| is_text(e, "welcome back"));
    assert!(removed < rejoined && rejoined < message, "{events:?}");
    let Some(Event::RemovedForInactivity { missed_since }) = events.get(removed) else {
        unreachable!()
    };
    assert!(*missed_since <= pi.now());
}

// spec: message-delivery/ordered-delivery-and-catch-up
// spec: group-membership/own-leaf-rotation
#[tokio::test(flavor = "multi_thread")]
async fn messages_around_an_own_commit_are_all_read() {
    let pi = TestPi::start().await;
    let (_admin, mut alice, mut bob) = chat(&pi).await;
    let epoch = alice.epoch().check("epoch");

    // Alice's leaf is due for rotation: it happens within the next hour of
    // her clock, while Bob keeps writing. Her jobs wait for 100 ms of quiet,
    // so Bob writes about that often: his messages land just before and
    // just after her commit.
    alice.clock.advance(8 * DAY);
    tokio::time::sleep(Duration::from_millis(300)).await;
    alice.clock.advance(Duration::from_secs(61 * 60));
    let mut ids = Vec::new();
    for n in 0..30 {
        ids.push(bob.client.send_text(&format!("b{n}")).check("send"));
        tokio::time::sleep(Duration::from_millis(95 + (n % 4) * 5)).await;
    }
    for id in ids {
        wait_accepted(&mut bob, id).await;
    }
    for n in 0..30 {
        wait_text(&mut alice, &format!("b{n}")).await;
    }
    alice.wait_epoch(epoch + 1).await;
    assert!(
        !alice
            .events()
            .iter()
            .any(|e| matches!(e, Event::Unreadable { .. })),
        "every message was read"
    );
    let expected: Vec<String> = (0..30).map(|n| format!("b{n}")).collect();
    assert_eq!(texts(&alice), expected);
}

// spec: message-delivery/envelope
// spec: client-runtime/events-in-order
#[tokio::test(flavor = "multi_thread")]
async fn a_message_whose_mls_sender_is_not_the_named_device_is_shown_as_unreadable() {
    let pi = TestPi::start().await;
    let (admin, mut alice, mut bob) = chat(&pi).await;
    go_offline(&pi, &bob).await;

    // A Pi that lies about the sender: Alice's message, stored as the
    // admin's.
    let id = alice.client.send_text("from alice").check("send");
    wait_accepted(&mut alice, id).await;
    let (alice_id, admin_id) = (alice.device_id(), admin.device_id());
    pi.query(move |db| {
        db.execute(
            "UPDATE envelopes SET sender_device_id = ?2 WHERE sender_device_id = ?1",
            rusqlite::params![alice_id.as_slice(), admin_id.as_slice()],
        )
    })
    .await;

    bob.client.start().await.check("start bob");
    let unreadable = bob
        .wait_event("the unreadable notice", |e| {
            matches!(e, Event::Unreadable { .. })
        })
        .await;
    let Event::Unreadable {
        sender,
        accepted_at,
    } = unreadable
    else {
        unreachable!()
    };
    assert_eq!(sender, Some(user(ADMIN)));
    assert!(accepted_at.is_some());
    assert!(texts(&bob).is_empty(), "nothing is shown as the admin's");

    // Bob goes on reading what follows.
    let id = alice.client.send_text("next").check("send");
    wait_accepted(&mut alice, id).await;
    let next = wait_text(&mut bob, "next").await;
    assert_eq!(next.sender, user("alice"));
}

// spec: client-runtime/send-status
// spec: group-membership/commit-authority
// spec: group-membership/inactive-devices-are-kept-out
#[tokio::test(flavor = "multi_thread")]
async fn a_member_commits_pending_pi_removes_before_sending() {
    let pi = TestPi::start().await;
    let (mut admin, mut alice, bob) = chat(&pi).await;
    let epoch = alice.epoch().check("epoch");
    go_offline(&pi, &bob).await;

    // Ten days pass on the Pi (Alice and the admin stay online), then the
    // admin's device goes away just before the Pi removes Bob's laptop, so
    // nobody commits the Pi's Remove proposal at once.
    pi.clock.advance(10 * DAY);
    for device in [&mut admin, &mut alice] {
        device
            .wait_event("the stream to be closed", |e| {
                matches!(e, Event::Connection(ConnectionState::Connecting))
            })
            .await;
        device.wait_connected().await;
    }
    go_offline(&pi, &admin).await;
    pi.sweep().await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(alice.epoch(), Some(epoch), "the Remove is only proposed");

    // Alice writes: OpenMLS encrypts nothing while a proposal is pending, so
    // her core commits the Pi's Remove first, then sends.
    let id = alice.client.send_text("still here").check("send");
    wait_accepted(&mut alice, id).await;
    assert_eq!(alice.epoch(), Some(epoch + 1));
    alice.wait_members(&[member(&admin), member(&alice)]).await;

    admin.client.start().await.check("start admin");
    wait_text(&mut admin, "still here").await;
    admin.wait_members(&[member(&admin), member(&alice)]).await;
}
