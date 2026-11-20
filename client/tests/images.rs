//! Image messages end to end against the real server (change `media-blobs`):
//! sending, receiving, opening, caching, expiry and resend, and that the server
//! learns nothing it should not (message-content: Image messages; client-runtime:
//! Image encode pipeline, Blob crypto and send order, Image download and
//! cache, Expired media).

mod support;

use std::io::Cursor;

use client::media::encode::webp_size;
use client::{
    ChatId, ClientError, Event, ImageBody, MediaState, Mention, MentionTarget, Message,
    MessageBody, MessageId, PushPlatform, PushToken, SendStatus,
};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use support::Check;
use support::device::{self, Device, member, user};
use support::server::TestServer;

/// The admin, Alice and Bob, all in the group and seeing each other.
async fn chat(server: &TestServer) -> (Device, Device, Device) {
    let admin = device::admin_with_group(server).await;
    let alice = device::member_of_group(&admin, "alice").await;
    let bob = device::member_of_group(&admin, "bob").await;
    let everyone = [member(&admin), member(&alice), member(&bob)];
    for device in [&admin, &alice, &bob] {
        device.wait_members(&everyone).await;
    }
    (admin, alice, bob)
}

/// A picture as a PNG file, as a picker could hand it over.
fn picture(w: u32, h: u32) -> Vec<u8> {
    let image = RgbaImage::from_fn(w, h, |x, y| {
        Rgba([(x * 255 / w) as u8, (y * 255 / h) as u8, 120, 255])
    });
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .check("png");
    out
}

fn image_of(message: &Message) -> &ImageBody {
    match &message.body {
        MessageBody::Image(image) => image,
        other => panic!("expected an image, got {other:?}"),
    }
}

async fn wait_image(device: &mut Device) -> Message {
    match device
        .wait_event(
            "an image message",
            |e| matches!(e, Event::Message(m) if matches!(m.body, MessageBody::Image(_))),
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

fn at_bob() -> Mention {
    Mention {
        target: MentionTarget::User(user("bob")),
        start: 0,
        len: 4,
    }
}

// spec: message-content/image-messages
// spec: message-content/image-caption-and-mentions
// spec: client-runtime/blob-crypto-and-send-order
// spec: client-runtime/image-download-and-cache
#[tokio::test(flavor = "multi_thread")]
async fn alice_sends_a_captioned_picture_and_bob_and_the_admin_open_it() {
    let server = TestServer::start().await;
    let (mut admin, mut alice, mut bob) = chat(&server).await;

    let id = alice
        .client
        .send_image(&picture(640, 480), Some("@Bob look"), &[at_bob()])
        .await
        .check("send_image");
    wait_accepted(&mut alice, id).await;
    // Uploaded before the message went out, and stored whole.
    assert_eq!(server.blobs().len(), 1);

    // Bob sees the blur's data and the caption, and is mentioned by it.
    let received = wait_image(&mut bob).await;
    let image = image_of(&received);
    assert_eq!((image.width, image.height), (640, 480));
    assert_eq!(image.caption.as_deref(), Some("@Bob look"));
    assert_eq!(image.state, MediaState::NotDownloaded);
    assert!(!image.thumbhash.is_empty());
    assert_eq!(received.mentions, [at_bob()]);
    assert!(received.mentions_me);
    assert_eq!(bob.client.unseen_mentions().check("unseen"), [received.id]);
    assert_eq!(
        bob.client.cached_image(received.id).await.check("cached"),
        None
    );

    // The sender has its own picture at once.
    let own = alice.client.message(id).check("message").check("some");
    assert_eq!(image_of(&own).state, MediaState::Available);
    assert!(
        alice
            .client
            .cached_image(id)
            .await
            .check("cached")
            .is_some()
    );

    // Bob opens it: downloaded, decrypted, a WebP of the size sent.
    let webp = bob
        .client
        .download_image(received.id)
        .await
        .check("download");
    assert_eq!(webp_size(&webp), Some((640, 480)));
    let now = bob
        .client
        .message(received.id)
        .check("message")
        .check("some");
    assert_eq!(image_of(&now).state, MediaState::Available);
    assert_eq!(server.blobs().len(), 1, "the admin still needs it");

    // Opened again with no connection: from the device's cache.
    bob.client.stop();
    assert_eq!(
        bob.client
            .download_image(received.id)
            .await
            .check("from cache"),
        webp
    );

    // The admin was the last to need it: the server deletes the blob.
    let at_admin = wait_image(&mut admin).await;
    let admin_webp = admin
        .client
        .download_image(at_admin.id)
        .await
        .check("download");
    assert_eq!(admin_webp, webp, "the same picture for everyone");
    assert!(server.blobs().is_empty(), "every member device has acked");
}

// spec: message-content/image-messages
#[tokio::test(flavor = "multi_thread")]
async fn a_picture_without_a_caption_is_just_a_picture() {
    let server = TestServer::start().await;
    let (_admin, mut alice, mut bob) = chat(&server).await;
    let id = alice
        .client
        .send_image(&picture(100, 100), None, &[])
        .await
        .check("send_image");
    wait_accepted(&mut alice, id).await;
    let received = wait_image(&mut bob).await;
    assert_eq!(image_of(&received).caption, None);
    assert!(received.mentions.is_empty() && !received.mentions_me);
}

async fn wait_failed(device: &mut Device, id: MessageId) -> String {
    match device
        .wait_event("the message failed", |e| {
            matches!(e, Event::SendStatus { id: i, status: SendStatus::Failed { .. } } if *i == id)
        })
        .await
    {
        Event::SendStatus {
            status: SendStatus::Failed { reason },
            ..
        } => reason,
        _ => unreachable!(),
    }
}

fn texts_and_images(device: &Device) -> Vec<&'static str> {
    device
        .client
        .history(100)
        .check("history")
        .iter()
        .map(|m| match m.body {
            MessageBody::Image(_) => "image",
            MessageBody::Text(_) => "text",
            _ => "other",
        })
        .collect()
}

// spec: client-runtime/blob-crypto-and-send-order
#[tokio::test(flavor = "multi_thread")]
async fn an_image_sent_offline_waits_in_the_outbox_and_its_blob_goes_up_first() {
    let server = TestServer::start().await;
    let (_admin, mut alice, mut bob) = chat(&server).await;
    alice.client.stop();
    server.wait_offline_recorded(&alice.device_id()).await;

    // Offline, an image is stored and queued like text: no error, shown as
    // sending, its picture already hers, nothing on the server.
    let id = alice
        .client
        .send_image(&picture(300, 200), Some("from the train"), &[])
        .await
        .check("send_image offline");
    let after = alice.client.send_text("and a line after it").check("send");
    let own = alice.client.message(id).check("message").check("some");
    assert_eq!(own.status, Some(SendStatus::Sending));
    assert_eq!(image_of(&own).state, MediaState::Available);
    assert!(
        server.blobs().is_empty(),
        "nothing is uploaded while offline"
    );

    // Back online: the blob goes up, then the message, then the text behind it.
    alice.client.start().await.check("start");
    wait_accepted(&mut alice, id).await;
    wait_accepted(&mut alice, after).await;
    assert_eq!(server.blobs().len(), 1);
    let received = wait_image(&mut bob).await;
    let webp = bob
        .client
        .download_image(received.id)
        .await
        .check("download");
    assert_eq!(webp_size(&webp), Some((300, 200)));
    assert_eq!(
        texts_and_images(&bob),
        ["image", "text"],
        "in the order sent"
    );
}

// spec: client-runtime/blob-crypto-and-send-order
#[tokio::test(flavor = "multi_thread")]
async fn a_queued_image_survives_a_crash_and_still_goes_out_whole() {
    let server = TestServer::start().await;
    let (_admin, mut alice, mut bob) = chat(&server).await;
    alice.client.stop();
    server.wait_offline_recorded(&alice.device_id()).await;
    let id = alice
        .client
        .send_image(&picture(200, 200), None, &[])
        .await
        .check("send_image offline");

    // The app is killed and opened again.
    alice.restart().await;
    wait_accepted(&mut alice, id).await;
    let received = wait_image(&mut bob).await;
    assert!(bob.client.download_image(received.id).await.is_ok());
    assert_eq!(server.blobs().len(), 1, "one blob, however it was queued");
}

// spec: client-runtime/blob-crypto-and-send-order
#[tokio::test(flavor = "multi_thread")]
async fn while_the_pis_drive_is_away_the_image_waits_and_nothing_is_sent() {
    let server = TestServer::start().await;
    let (_admin, mut alice, mut bob) = chat(&server).await;
    // The server's drive is pulled: its blob store answers UNAVAILABLE.
    let away = server.blob_dir().with_extension("away");
    std::fs::rename(server.blob_dir(), &away).check("pull the drive");

    let id = alice
        .client
        .send_image(&picture(120, 90), None, &[])
        .await
        .check("send_image");
    // No envelope is sent without its blob, so Bob sees nothing.
    assert!(
        !bob.sees_within_quiet(
            |e| matches!(e, Event::Message(m) if matches!(m.body, MessageBody::Image(_)))
        )
        .await
    );
    let own = alice.client.message(id).check("message").check("some");
    assert_eq!(
        own.status,
        Some(SendStatus::Sending),
        "still waiting, not failed"
    );

    // The drive is back: the next try uploads it and the message goes out.
    std::fs::rename(&away, server.blob_dir()).check("mount the drive");
    wait_accepted(&mut alice, id).await;
    let received = wait_image(&mut bob).await;
    assert!(bob.client.download_image(received.id).await.is_ok());
}

// spec: client-runtime/blob-crypto-and-send-order
#[tokio::test(flavor = "multi_thread")]
async fn a_queued_image_whose_picture_is_gone_fails_and_the_chat_goes_on() {
    let server = TestServer::start().await;
    let (_admin, mut alice, mut bob) = chat(&server).await;
    alice.client.stop();
    server.wait_offline_recorded(&alice.device_id()).await;
    let id = alice
        .client
        .send_image(&picture(100, 100), None, &[])
        .await
        .check("send_image offline");
    let text = alice.client.send_text("still here").check("send");
    // The app's cache was cleared while the image waited.
    std::fs::remove_dir_all(alice.data_dir().join("media")).check("clear the cache");

    alice.client.start().await.check("start");
    let reason = wait_failed(&mut alice, id).await;
    assert!(reason.contains("no longer on this device"), "{reason}");
    // Nothing was sent for it, and the text behind it was not lost.
    assert!(server.blobs().is_empty());
    wait_accepted(&mut alice, text).await;
    let received = wait_text_in(&mut bob, "still here").await;
    assert_eq!(received.body, MessageBody::Text("still here".to_owned()));
    assert_eq!(texts_and_images(&bob), ["text"]);
}

// spec: client-runtime/blob-crypto-and-send-order
#[tokio::test(flavor = "multi_thread")]
async fn an_image_that_waited_longer_than_a_blob_lives_is_not_sent() {
    let server = TestServer::start().await;
    let (_admin, mut alice, _bob) = chat(&server).await;
    alice.client.stop();
    server.wait_offline_recorded(&alice.device_id()).await;
    let id = alice
        .client
        .send_image(&picture(100, 100), None, &[])
        .await
        .check("send_image offline");

    // Eight days offline: the policy is that such a picture is stale.
    alice
        .clock
        .advance(std::time::Duration::from_secs(8 * 24 * 60 * 60));
    alice.client.start().await.check("start");
    let reason = wait_failed(&mut alice, id).await;
    assert!(reason.contains("in time"), "{reason}");
    assert!(server.blobs().is_empty());
}

async fn wait_text_in(device: &mut Device, text: &str) -> Message {
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

// spec: client-runtime/image-encode-pipeline
// spec: message-content/image-caption-and-mentions
#[tokio::test(flavor = "multi_thread")]
async fn a_bad_picture_or_caption_is_refused_before_anything_is_uploaded() {
    let server = TestServer::start().await;
    let (_admin, alice, _bob) = chat(&server).await;

    let not_an_image = alice.client.send_image(b"hello", None, &[]).await;
    assert!(
        matches!(not_an_image, Err(ClientError::Media(_))),
        "{not_an_image:?}"
    );

    let bad_span = Mention {
        target: MentionTarget::Everyone,
        start: 1,
        len: 9,
    };
    let refused = alice
        .client
        .send_image(&picture(50, 50), Some("hi"), &[bad_span])
        .await;
    assert!(
        matches!(refused, Err(ClientError::InvalidMention)),
        "{refused:?}"
    );

    let too_long = "a".repeat(70_000);
    let refused = alice
        .client
        .send_image(&picture(50, 50), Some(&too_long), &[])
        .await;
    assert!(matches!(refused, Err(ClientError::TooLong)), "{refused:?}");

    assert!(server.blobs().is_empty(), "nothing was uploaded");
    assert!(alice.client.history(100).check("history").is_empty());
}

// spec: client-runtime/expired-media
#[tokio::test(flavor = "multi_thread")]
async fn an_expired_picture_shows_as_expired_and_the_sender_can_resend_it() {
    let server = TestServer::start().await;
    let (_admin, mut alice, mut bob) = chat(&server).await;
    let id = alice
        .client
        .send_image(&picture(320, 240), Some("old news"), &[])
        .await
        .check("send_image");
    wait_accepted(&mut alice, id).await;
    let received = wait_image(&mut bob).await;
    let first_blob = image_of(&received).blob_id;

    // The server's 7 days are up. (The server's own tests move its clock; here the
    // blob's expiry is brought forward, so no session token runs out too.)
    server
        .query(|db| db.execute("UPDATE blobs SET expires_at = 1", []))
        .await;
    server.sweep().await;
    assert!(server.blobs().is_empty(), "the server deleted it");

    // Bob finds out when he opens it, and is told so from then on.
    let opened = bob.client.download_image(received.id).await;
    assert!(
        matches!(opened, Err(ClientError::MediaExpired)),
        "{opened:?}"
    );
    let now = bob
        .client
        .message(received.id)
        .check("message")
        .check("some");
    assert_eq!(image_of(&now).state, MediaState::Expired);
    assert!(
        !image_of(&now).thumbhash.is_empty(),
        "the blur is in the message, so it stays"
    );
    bob.client.stop();
    let again = bob.client.download_image(received.id).await;
    assert!(
        matches!(again, Err(ClientError::MediaExpired)),
        "no network needed to know: {again:?}"
    );
    bob.client.start().await.check("start");

    // Alice kept the picture: resending makes a new message with a new blob.
    let again_id = alice.client.resend_image(id).await.check("resend");
    assert_ne!(again_id, id);
    wait_accepted(&mut alice, again_id).await;
    let resent = wait_image(&mut bob).await;
    let image = image_of(&resent);
    assert_ne!(image.blob_id, first_blob, "a fresh blob");
    assert_eq!(image.caption.as_deref(), Some("old news"));
    let webp = bob.client.download_image(resent.id).await.check("download");
    assert_eq!(webp_size(&webp), Some((320, 240)));
}

// spec: client-runtime/expired-media
#[tokio::test(flavor = "multi_thread")]
async fn only_a_picture_this_device_has_can_be_resent() {
    let server = TestServer::start().await;
    let (_admin, mut alice, mut bob) = chat(&server).await;
    let id = alice
        .client
        .send_image(&picture(64, 64), None, &[])
        .await
        .check("send_image");
    wait_accepted(&mut alice, id).await;
    let received = wait_image(&mut bob).await;
    // Bob did not send it.
    let refused = bob.client.resend_image(received.id).await;
    assert!(
        matches!(refused, Err(ClientError::NoSuchMessage)),
        "{refused:?}"
    );
    // A text is not a picture.
    let text = alice.client.send_text("hello").check("send");
    let refused = alice.client.resend_image(text).await;
    assert!(
        matches!(refused, Err(ClientError::NoSuchMessage)),
        "{refused:?}"
    );
}

// spec: message-content/image-messages-stay-encrypted
#[tokio::test(flavor = "multi_thread")]
async fn the_server_sees_ciphertext_and_wakes_bob_like_for_any_message() {
    let server = TestServer::start().await;
    let (_admin, mut alice, bob) = chat(&server).await;
    bob.client
        .set_push_token(Some(PushToken {
            platform: PushPlatform::Fcm,
            token: "bob-token".into(),
        }))
        .await
        .check("token");
    bob.client.stop();
    server.wait_offline_recorded(&bob.device_id()).await;

    let id = alice
        .client
        .send_image(&picture(200, 100), Some("a secret caption"), &[])
        .await
        .check("send_image");
    wait_accepted(&mut alice, id).await;

    // One ping: Bob's id and token, and the providers' fixed "New message".
    let bob_id = bob.device_id();
    let deadline = tokio::time::Instant::now() + support::server::WAIT;
    while server.pushes.count(&bob_id) == 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "no wake ping for Bob"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(server.pushes.sent(), [(bob_id, "bob-token".to_owned())]);

    // The envelope holds neither the caption nor any picture bytes.
    let rows: Vec<Vec<u8>> = server
        .query(|db| {
            let mut statement = db.prepare("SELECT ciphertext FROM envelopes WHERE kind = 1")?;
            statement.query_map([], |row| row.get(0))?.collect()
        })
        .await;
    assert!(!rows.is_empty());
    for row in rows {
        assert!(!support::contains(&row, b"a secret caption"));
        assert!(!support::contains(&row, b"WEBP"));
    }

    // The blob is ciphertext: no WebP header, nothing readable.
    let blobs = server.blobs();
    assert_eq!(blobs.len(), 1);
    let bytes = std::fs::read(server.blob_dir().join(&blobs[0])).check("blob file");
    assert!(!support::contains(&bytes, b"RIFF"));
    assert!(!support::contains(&bytes, b"WEBP"));
    assert!(!support::contains(&bytes, b"a secret caption"));
}

// spec: message-content/image-messages
#[tokio::test(flavor = "multi_thread")]
async fn a_picture_in_a_dm_reaches_only_the_peer() {
    let server = TestServer::start().await;
    let (mut admin, mut alice, mut bob) = chat(&server).await;
    let dm: ChatId = alice.client.open_dm(&user("bob")).await.check("open DM");
    alice
        .wait_until("Bob's devices in the DM", |c| {
            c.chats().is_ok_and(|chats| {
                chats.iter().any(|chat| {
                    chat.id == dm
                        && matches!(chat.kind, client::ChatKind::Dm { waiting: false, .. })
                })
            })
        })
        .await;

    let id = alice
        .client
        .send_image_in(dm, &picture(120, 90), Some("just us"), &[])
        .await
        .check("send_image_in");
    wait_accepted(&mut alice, id).await;
    let received = wait_image(&mut bob).await;
    assert_eq!(
        received.chat,
        bob.client
            .chats()
            .check("chats")
            .iter()
            .find(|c| matches!(c.kind, client::ChatKind::Dm { .. }))
            .check("dm")
            .id
    );
    let webp = bob
        .client
        .download_image(received.id)
        .await
        .check("download");
    assert_eq!(webp_size(&webp), Some((120, 90)));

    // The admin is not in the DM: it saw nothing, and cannot read the blob.
    assert!(
        !admin
            .sees_within_quiet(
                |e| matches!(e, Event::Message(m) if matches!(m.body, MessageBody::Image(_)))
            )
            .await
    );
}
