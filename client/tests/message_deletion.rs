//! Deleting messages end to end against the real server (change
//! `delete-messages`): the tombstone that never erases history, the DM reveal,
//! deleting media (with the blob purged) and replies to a deleted message
//! (message-deletion; blob-store: Delete a blob).

mod support;

use std::io::Cursor;
use std::time::Duration;

use client::{ChatId, ClientError, Event, Message, MessageBody, MessageId, SendStatus};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use support::Check;
use support::device::{self, Device, member, user};
use support::server::TestServer;

/// The admin, Alice and Bob, all in the group and seeing each other.
async fn group(server: &TestServer) -> (Device, Device, Device) {
    let admin = device::admin_with_group(server).await;
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

async fn wait_image(device: &mut Device) -> Message {
    match device
        .wait_event(
            "an image",
            |e| matches!(e, Event::Message(m) if matches!(m.body, MessageBody::Image(_))),
        )
        .await
    {
        Event::Message(message) => message,
        _ => unreachable!(),
    }
}

/// Waits for `id` to become a tombstone on `device`.
async fn wait_tombstone(device: &mut Device, id: MessageId) -> (bool, bool) {
    let message = match device
        .wait_event("a tombstone", |e| {
            matches!(e, Event::Message(m)
                if m.id == id && matches!(m.body, MessageBody::Deleted { .. }))
        })
        .await
    {
        Event::Message(message) => message,
        _ => unreachable!(),
    };
    match message.body {
        MessageBody::Deleted {
            by_admin,
            revealable,
        } => (by_admin, revealable),
        _ => unreachable!(),
    }
}

async fn wait_accepted(device: &mut Device, id: MessageId) {
    device
        .wait_event(
            "accepted",
            |e| matches!(e, Event::SendStatus { id: i, status: SendStatus::Accepted } if *i == id),
        )
        .await;
}

fn picture(w: u32, h: u32) -> Vec<u8> {
    let image = RgbaImage::from_fn(w, h, |x, y| {
        Rgba([(x * 255 / w) as u8, (y * 255 / h) as u8, 120, 255])
    });
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .check("encode png");
    out
}

// spec: message-deletion/tombstone-not-erasure
#[tokio::test(flavor = "multi_thread")]
async fn an_admin_delete_shows_a_tombstone_in_place_and_a_member_cannot_delete() {
    let server = TestServer::start().await;
    let (mut admin, alice, mut bob) = group(&server).await;

    alice.client.send_text("before").check("send");
    let alice_middle = alice.client.send_text("middle").check("send");
    alice.client.send_text("after").check("send");

    wait_text(&mut bob, "before").await;
    let bob_middle = wait_text(&mut bob, "middle").await;
    wait_text(&mut bob, "after").await;
    let admin_middle = wait_text(&mut admin, "middle").await;

    // A regular member cannot delete in a group, even their own message.
    assert!(matches!(
        alice.client.delete_message(ChatId::MAIN, alice_middle),
        Err(ClientError::CannotDelete)
    ));

    // The admin deletes Alice's message: everyone sees "by the admin", no reveal.
    admin
        .client
        .delete_message(ChatId::MAIN, admin_middle.id)
        .check("admin delete");
    let (by_admin, revealable) = wait_tombstone(&mut bob, bob_middle.id).await;
    assert!(by_admin);
    assert!(!revealable);

    // The tombstone keeps its place; the others are unmoved.
    let bodies: Vec<MessageBody> = bob
        .client
        .history(10)
        .check("history")
        .into_iter()
        .map(|m| m.body)
        .collect();
    assert_eq!(
        bodies,
        vec![
            MessageBody::Text("before".into()),
            MessageBody::Deleted {
                by_admin: true,
                revealable: false,
            },
            MessageBody::Text("after".into()),
        ]
    );
}

// spec: message-deletion/tombstone-not-erasure
#[tokio::test(flavor = "multi_thread")]
async fn an_admin_deleting_its_own_message_is_not_marked_by_the_admin() {
    let server = TestServer::start().await;
    let (admin, _alice, mut bob) = group(&server).await;
    let admins = admin.client.send_text("admin note").check("send");
    let bobs = wait_text(&mut bob, "admin note").await;
    admin
        .client
        .delete_message(ChatId::MAIN, admins)
        .check("delete own");
    let (by_admin, revealable) = wait_tombstone(&mut bob, bobs.id).await;
    assert!(!by_admin);
    assert!(!revealable);
}

// spec: message-deletion/receiver-may-reveal-in-a-dm
#[tokio::test(flavor = "multi_thread")]
async fn a_dm_delete_leaves_a_reveal_able_tombstone() {
    let server = TestServer::start().await;
    let (_admin, mut alice, mut bob) = group(&server).await;

    let chat = alice.client.open_dm(&user("bob")).await.check("open DM");
    let secret = alice
        .client
        .send_message_in(chat, "secret", &[], None)
        .check("send");
    wait_accepted(&mut alice, secret).await;
    let bob_secret = wait_text(&mut bob, "secret").await;

    // Alice deletes her own DM message.
    alice
        .client
        .delete_message(chat, secret)
        .check("delete own DM message");

    // Bob's tombstone offers Reveal, and Reveal shows the original he kept.
    let (by_admin, revealable) = wait_tombstone(&mut bob, bob_secret.id).await;
    assert!(!by_admin);
    assert!(revealable);
    let revealed = bob
        .client
        .reveal(bob_secret.chat, bob_secret.id)
        .check("reveal");
    assert_eq!(revealed.body, MessageBody::Text("secret".into()));
}

// spec: message-deletion/deleting-media-and-replies
// spec: blob-store/delete-a-blob
#[tokio::test(flavor = "multi_thread")]
async fn deleting_an_image_purges_its_blob() {
    let server = TestServer::start().await;
    let (mut admin, mut alice, _bob) = group(&server).await;

    let id = alice
        .client
        .send_image(&picture(64, 64), Some("a photo"), &[])
        .await
        .check("send image");
    wait_accepted(&mut alice, id).await;
    let admin_image = wait_image(&mut admin).await;
    assert_eq!(server.blobs().len(), 1, "the blob is stored");

    // The admin deletes the image; its blob is purged at once.
    admin
        .client
        .delete_message(ChatId::MAIN, admin_image.id)
        .check("delete image");
    wait_tombstone(&mut admin, admin_image.id).await;
    let mut gone = false;
    for _ in 0..100 {
        if server.blobs().is_empty() {
            gone = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(gone, "the blob was purged");
}

// spec: message-deletion/deleting-media-and-replies
#[tokio::test(flavor = "multi_thread")]
async fn a_reply_to_a_deleted_message_shows_a_deleted_quote() {
    let server = TestServer::start().await;
    let (mut admin, alice, mut bob) = group(&server).await;

    alice.client.send_text("original").check("send");
    let admin_orig = wait_text(&mut admin, "original").await;
    let bob_orig = wait_text(&mut bob, "original").await;

    // Bob replies quoting Alice's message.
    bob.client
        .send_message("re: original", &[], Some(bob_orig.id))
        .check("reply");
    wait_text(&mut admin, "re: original").await;

    // The admin deletes the original.
    admin
        .client
        .delete_message(ChatId::MAIN, admin_orig.id)
        .check("delete");
    wait_tombstone(&mut bob, bob_orig.id).await;

    // On Bob, the reply now quotes a "deleted message" (its original is gone).
    let reply = bob
        .client
        .history(10)
        .check("history")
        .into_iter()
        .find(|m| m.body == MessageBody::Text("re: original".into()))
        .check("the reply");
    assert!(reply.reply.check("a reply reference").original_deleted);
}
