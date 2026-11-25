//! Stickers end to end against the real server (change `photo-sticker`): a
//! sticker is an ordinary transparent `Image`, so it rides the unchanged image
//! blob path and a client with no sticker feature sees a plain transparent
//! image (photo-sticker: Sticker is an image; Segmentation seam and RGBA
//! contract). Only the core produces the WebP from the RGBA cutout; the send
//! path is the image one.

mod support;

use client::media::encode::webp_size;
use client::media::{RgbaCutout, SegmentationSource};
use client::{ChatId, Event, ImageBody, MediaState, Message, MessageBody, MessageId, SendStatus};
use image::ImageFormat;
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

/// A subject cutout as a `SegmentationSource` would hand it to the core: an
/// opaque coloured disc centred in a mostly-transparent `edge`×`edge` canvas,
/// straight alpha, R,G,B,A byte order, stride `edge`×4. A disc (not a rectangle)
/// so transparent background survives the core's trim, as a real cutout does.
fn sticker_cutout(edge: u32) -> RgbaCutout {
    let (c, r) = (edge as f32 / 2.0, edge as f32 / 2.0 * 0.6);
    let mut pixels = vec![0u8; (edge * edge * 4) as usize];
    for y in 0..edge {
        for x in 0..edge {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            if dx * dx + dy * dy <= r * r {
                let i = ((y * edge + x) * 4) as usize;
                pixels[i] = 30;
                pixels[i + 1] = 160;
                pixels[i + 2] = 220;
                pixels[i + 3] = 255;
            }
        }
    }
    RgbaCutout {
        source: SegmentationSource::IosVision,
        width: edge,
        height: edge,
        pixels,
    }
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

// spec: photo-sticker/sticker-is-an-image
// spec: photo-sticker/on-device-native-cutout
#[tokio::test(flavor = "multi_thread")]
async fn alice_sends_a_sticker_and_bob_sees_a_transparent_image() {
    let server = TestServer::start().await;
    let (_admin, mut alice, mut bob) = chat(&server).await;

    let id = alice
        .client
        .send_sticker(sticker_cutout(400))
        .await
        .check("send_sticker");
    wait_accepted(&mut alice, id).await;
    // It went out as one ordinary image blob, upload-then-send.
    assert_eq!(server.blobs().len(), 1);

    // Bob — who need know nothing of stickers — receives a plain `Image` body,
    // never an error or a new body type.
    let received = wait_image(&mut bob).await;
    let image = image_of(&received);
    assert_eq!(image.caption, None);
    assert!(received.mentions.is_empty());
    assert_eq!(image.state, MediaState::NotDownloaded);
    assert!(!image.thumbhash.is_empty());

    // He opens it: a WebP that keeps its alpha, so the background is transparent.
    let webp = bob
        .client
        .download_image(received.id)
        .await
        .check("download");
    assert_eq!(webp_size(&webp), Some((image.width, image.height)));
    let decoded = image::load_from_memory_with_format(&webp, ImageFormat::WebP)
        .check("decode")
        .to_rgba8();
    assert!(
        decoded.pixels().any(|px| px[3] < 20),
        "the sticker's background is transparent"
    );
    assert!(
        decoded.pixels().any(|px| px[3] > 200),
        "the subject is opaque"
    );
}

// spec: photo-sticker/sticker-is-an-image
#[tokio::test(flavor = "multi_thread")]
async fn a_sticker_is_sendable_in_a_dm_like_an_image() {
    let server = TestServer::start().await;
    let (_admin, alice, mut bob) = chat(&server).await;

    let chat: ChatId = alice.client.open_dm(&user("bob")).await.check("open DM");
    let id = alice
        .client
        .send_sticker_in(chat, sticker_cutout(300))
        .await
        .check("send_sticker_in");

    // Bob receives it in the DM, as an ordinary image.
    let received = match bob
        .wait_event("a sticker in the DM", |e| {
            matches!(e, Event::Message(m) if m.chat != ChatId::MAIN && matches!(m.body, MessageBody::Image(_)))
        })
        .await
    {
        Event::Message(message) => message,
        _ => unreachable!(),
    };
    assert!(matches!(received.body, MessageBody::Image(_)));
    let _ = id;
}
