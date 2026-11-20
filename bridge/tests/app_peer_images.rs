//! The host side of the app's picture integration test (change `media-blobs`):
//! the admin's device, driven through the bridge's `Core` against the temp server
//! that `app/integration_test/run.sh images` starts. Ignored by `cargo test`.
//!
//! With the app (Bob) on the other side it:
//!
//! 1. registers the admin, creates the group and an invite for `bob`
//!    (`$VYAJENA_PEER_DIR/invite.txt`); Bob's phone is added on its own;
//! 2. receives Bob's picture captioned "cat" and opens it (download, decrypt);
//! 3. sends a picture captioned "from admin" for Bob to open;
//! 4. receives Bob's picture "queued", which Bob sent while offline (the
//!    upload went first, then the message), and opens it;
//! 5. receives Bob's second "cat", the "Resend picture" of the first, and
//!    opens it; then writes `done`.

mod support;

use std::io::Cursor;
use std::time::Duration;

use bridge::api::types::{ChatBody, CoreEvent, SendState};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use support::Check;
use support::device::{Device, user};
use support::server::ADMIN;

/// How long the peer waits for the app: building and installing the app
/// on a simulator takes minutes.
const APP_WAIT: Duration = Duration::from_secs(15 * 60);

fn env(name: &str) -> String {
    std::env::var(name)
        .unwrap_or_else(|_| panic!("{name} is not set (run app/integration_test/run.sh images)"))
}

/// Writes `text` to `path` in one step (the script polls for the file).
fn publish(path: &std::path::Path, text: &str) {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text).check("write");
    std::fs::rename(&tmp, path).check("rename");
}

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

/// Waits for an image from Bob with this caption and opens it.
async fn open_bobs_picture(admin: &mut Device, caption: &str) {
    let event = admin
        .wait_event_for(&format!("bob's picture \"{caption}\""), APP_WAIT, |e| {
            matches!(e, CoreEvent::Message { message }
                if message.sender == user("bob")
                    && matches!(&message.body, ChatBody::Image { image }
                        if image.caption.as_deref() == Some(caption)))
        })
        .await;
    let CoreEvent::Message { message } = event else {
        unreachable!()
    };
    let bytes = admin
        .core
        .download_image(message.id)
        .await
        .check("download bob's picture");
    assert_eq!(&bytes[..4], b"RIFF", "a WebP came back");
    assert_eq!(&bytes[8..12], b"WEBP", "a WebP came back");
    eprintln!("app_peer_images: opened bob's \"{caption}\"");
}

// spec: app-ui/send-an-image
// spec: app-ui/image-bubble-and-viewer
// spec: client-runtime/blob-crypto-and-send-order
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the host side of app/integration_test/images_test.dart; run by run.sh images"]
async fn app_peer_images() {
    support::init_logging();
    let bootstrap = env("VYAJENA_PEER_BOOTSTRAP");
    let out = std::path::PathBuf::from(env("VYAJENA_PEER_DIR"));

    let mut admin = Device::on_system_clock().await;
    let id = admin.core.register(bootstrap).await.check("register admin");
    assert_eq!(id, user(ADMIN));
    admin.core.start().await.check("start admin");
    admin.wait_connected().await;
    admin.core.create_group().await.check("create group");
    let invite = admin
        .core
        .create_invite("bob".to_owned())
        .await
        .check("create invite");
    publish(&out.join("invite.txt"), &invite.text.check("invite text"));
    eprintln!("app_peer_images: invite for bob written; waiting for the app");

    admin
        .wait_until_for("bob's phone added", APP_WAIT, |c| {
            c.members()
                .is_ok_and(|m| m.iter().any(|d| d.user == user("bob")))
        })
        .await;

    open_bobs_picture(&mut admin, "cat").await;

    let sent = admin
        .core
        .send_image(png(240, 160), Some("from admin".to_owned()), Vec::new())
        .await
        .check("send picture");
    admin
        .wait_event("picture accepted", |e| {
            matches!(e, CoreEvent::SendStatus { id, status: SendState::Accepted } if *id == sent)
        })
        .await;
    eprintln!("app_peer_images: sent \"from admin\"");
    publish(&out.join("admin-picture-sent"), "sent");

    open_bobs_picture(&mut admin, "queued").await;
    // The first "cat" was seen above; the resend is a second message.
    admin
        .wait_until_for("bob's resent \"cat\"", APP_WAIT, |core| {
            core.history(100).is_ok_and(|all| {
                all.iter()
                    .filter(|m| {
                        matches!(&m.body, ChatBody::Image { image }
                            if image.caption.as_deref() == Some("cat"))
                    })
                    .count()
                    >= 2
            })
        })
        .await;
    eprintln!("app_peer_images: both \"cat\" pictures arrived");
    publish(&out.join("done"), "done");
}
