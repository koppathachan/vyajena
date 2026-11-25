//! Stickers through the bridge (change `photo-sticker`): Dart hands the core
//! the straight-alpha RGBA cutout its platform `SegmentationSource` produced,
//! the core encodes it as a transparent WebP and sends it down the image path,
//! and the message comes back as an ordinary image with no key and no blob id
//! in what Dart holds (photo-sticker: Sticker is an image; Sticker styling and
//! encode).

mod support;

use bridge::api::core::thumbhash_is_transparent;
use bridge::api::types::{ChatBody, ChatMessage, CoreEvent, ImageView, SendState, StickerSource};
use image::ImageFormat;
use support::Check;
use support::device::{self, Device, user};
use support::server::TestServer;

/// The RGBA cutout a platform plugin hands the bridge: an opaque coloured disc
/// in a transparent `edge`×`edge` canvas, R,G,B,A byte order, stride `edge`×4.
fn cutout(edge: u32) -> Vec<u8> {
    let (c, r) = (edge as f32 / 2.0, edge as f32 / 2.0 * 0.6);
    let mut pixels = vec![0u8; (edge * edge * 4) as usize];
    for y in 0..edge {
        for x in 0..edge {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            if dx * dx + dy * dy <= r * r {
                let i = ((y * edge + x) * 4) as usize;
                pixels[i] = 220;
                pixels[i + 1] = 60;
                pixels[i + 2] = 120;
                pixels[i + 3] = 255;
            }
        }
    }
    pixels
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

// spec: photo-sticker/sticker-is-an-image
// spec: photo-sticker/segmentation-seam
#[tokio::test(flavor = "multi_thread")]
async fn a_cutout_from_dart_is_sent_as_a_transparent_image() {
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

    let edge = 400;
    let id = alice
        .core
        .send_sticker_in(0, StickerSource::IosVision, cutout(edge), edge, edge)
        .await
        .check("send_sticker_in");
    alice
        .wait_event("accepted", |e| {
            matches!(e, CoreEvent::SendStatus { id: i, status: SendState::Accepted } if *i == id)
        })
        .await;

    // It arrives as an ordinary image body — no sticker body type, no caption.
    let received = wait_image(&mut bob).await;
    let image = image_of(&received);
    assert_eq!(image.caption, None);
    assert!(!image.thumbhash.is_empty());
    // The core's ThumbHash marks it transparent, so the app renders it without
    // an opaque bubble fill.
    assert!(thumbhash_is_transparent(image.thumbhash.clone()));
    assert!(!thumbhash_is_transparent(vec![1, 2, 3]));

    // Bob downloads a WebP that kept its alpha: a transparent background.
    let bytes = bob.core.download_image(received.id).await.check("download");
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
    let decoded = image::load_from_memory_with_format(&bytes, ImageFormat::WebP)
        .check("decode")
        .to_rgba8();
    assert!(
        decoded.pixels().any(|px| px[3] < 20),
        "transparent background"
    );
    assert!(decoded.pixels().any(|px| px[3] > 200), "opaque subject");
}
