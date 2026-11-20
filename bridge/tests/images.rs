//! Image messages through the bridge (change `media-blobs`): Dart's picked
//! bytes and UTF-16 caption ranges reach the core, and the message comes back
//! with its blur data, caption ranges and where the picture is, with no key
//! and no blob id in what Dart holds.

mod support;

use std::io::Cursor;

use bridge::api::error::CoreErrorKind;
use bridge::api::types::{
    ChatBody, ChatMessage, CoreEvent, ImageView, MediaStatus, MentionSpan, MentionTarget, SendState,
};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use support::Check;
use support::device::{self, Device, user};
use support::server::TestServer;

fn png(w: u32, h: u32) -> Vec<u8> {
    let image = RgbaImage::from_fn(w, h, |x, y| {
        Rgba([(x * 255 / w) as u8, (y * 255 / h) as u8, 90, 255])
    });
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .check("png");
    out
}

async fn wait_image(device: &mut Device) -> ChatMessage {
    let event = device
        .wait_event("an image message", |e| {
            matches!(e, CoreEvent::Message { message }
                if matches!(message.body, ChatBody::Image { .. }))
        })
        .await;
    match event {
        CoreEvent::Message { message } => message,
        _ => unreachable!(),
    }
}

fn image_of(message: &ChatMessage) -> &ImageView {
    match &message.body {
        ChatBody::Image { image } => image,
        other => panic!("expected an image, got {other:?}"),
    }
}

// spec: app-ui/send-an-image
// spec: app-ui/image-bubble-and-viewer
#[tokio::test(flavor = "multi_thread")]
async fn a_caption_with_an_emoji_keeps_its_utf16_ranges_and_the_picture_opens() {
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let mut alice = device::member_of_group(&admin, "alice").await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    for device in [&alice, &bob] {
        device
            .wait_until("both users in the group", |c| {
                c.members().is_ok_and(|m| {
                    m.iter().any(|d| d.user == user("alice"))
                        && m.iter().any(|d| d.user == user("bob"))
                })
            })
            .await;
    }

    // "👋 @Bobby": the mention starts at UTF-16 unit 3 (the emoji is 2).
    let caption = "👋 @Bobby look";
    let at_bob = MentionSpan {
        target: MentionTarget::User { user: user("bob") },
        start: 3,
        length: 6,
    };
    // A range that splits the emoji is refused before anything is uploaded.
    let split = MentionSpan {
        target: MentionTarget::Everyone,
        start: 1,
        length: 2,
    };
    let refused = alice
        .core
        .send_image(png(80, 60), Some(caption.to_owned()), vec![split])
        .await;
    assert_eq!(
        refused.err().map(|e| e.kind),
        Some(CoreErrorKind::InvalidMention)
    );

    let id = alice
        .core
        .send_image(png(80, 60), Some(caption.to_owned()), vec![at_bob.clone()])
        .await
        .check("send_image");
    alice
        .wait_event("accepted", |e| {
            matches!(e, CoreEvent::SendStatus { id: i, status: SendState::Accepted } if *i == id)
        })
        .await;

    let received = wait_image(&mut bob).await;
    let image = image_of(&received);
    assert_eq!((image.width, image.height), (80, 60));
    assert_eq!(image.caption.as_deref(), Some(caption));
    assert_eq!(image.state, MediaStatus::NotDownloaded);
    assert!(!image.thumbhash.is_empty());
    // The same UTF-16 range comes back, over the caption.
    assert_eq!(received.mentions, [at_bob]);
    assert!(received.mentions_me);

    // The sender has its own picture; Bob downloads his.
    let own = alice.core.message(id).check("message").check("some");
    assert_eq!(image_of(&own).state, MediaStatus::Available);
    let bytes = bob.core.download_image(received.id).await.check("download");
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
    let again = bob.core.message(received.id).check("message").check("some");
    assert_eq!(image_of(&again).state, MediaStatus::Available);
    assert_eq!(
        bob.core.cached_image(received.id).await.check("cached"),
        Some(bytes)
    );
}

// spec: app-ui/send-an-image
#[tokio::test(flavor = "multi_thread")]
async fn a_file_that_is_not_an_image_is_refused_with_its_own_error() {
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let alice = device::member_of_group(&admin, "alice").await;
    let refused = alice
        .core
        .send_image(b"not a picture".to_vec(), None, Vec::new())
        .await;
    assert_eq!(
        refused.err().map(|e| e.kind),
        Some(CoreErrorKind::ImageUnsupported)
    );
}

// spec: app-ui/image-bubble-and-viewer
#[tokio::test(flavor = "multi_thread")]
async fn an_expired_picture_is_its_own_error_and_state_and_the_sender_resends() {
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let alice = device::member_of_group(&admin, "alice").await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    for device in [&alice, &bob] {
        device
            .wait_until("both users in the group", |c| {
                c.members().is_ok_and(|m| {
                    m.iter().any(|d| d.user == user("alice"))
                        && m.iter().any(|d| d.user == user("bob"))
                })
            })
            .await;
    }
    let id = alice
        .core
        .send_image(png(64, 64), None, Vec::new())
        .await
        .check("send_image");
    let received = wait_image(&mut bob).await;

    server
        .query(|db| db.execute("UPDATE blobs SET expires_at = 1", []))
        .await;
    server.sweep().await;

    let gone = bob.core.download_image(received.id).await;
    assert_eq!(
        gone.err().map(|e| e.kind),
        Some(CoreErrorKind::MediaExpired)
    );
    let now = bob.core.message(received.id).check("message").check("some");
    assert_eq!(image_of(&now).state, MediaStatus::Expired);

    // Bob cannot resend what he did not send.
    let not_mine = bob.core.resend_image(received.id).await;
    assert_eq!(
        not_mine.err().map(|e| e.kind),
        Some(CoreErrorKind::NoSuchMessage)
    );
    let again = alice.core.resend_image(id).await.check("resend");
    assert_ne!(again, id);
    let resent = wait_image(&mut bob).await;
    assert!(bob.core.download_image(resent.id).await.is_ok());
}
