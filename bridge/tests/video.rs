//! Video messages through the bridge (change `video-messages`): the app's
//! already-normalized MP4 reaches the core, and the message comes back with its
//! dimensions, duration and first-frame blur, with no key and no blob id in what
//! Dart holds; the player is handed a file path, never the bytes.

mod support;

use bridge::api::error::CoreErrorKind;
use bridge::api::types::{ChatBody, ChatMessage, CoreEvent, MediaStatus, SendState, VideoView};
use support::Check;
use support::device::{self, Device, user};
use support::server::TestServer;

/// Bytes that look like a normalized MP4 (an `ftyp` box), standing in for what
/// the native plugin hands the bridge; the core never demuxes it.
fn mp4(len: usize) -> Vec<u8> {
    let mut out = vec![0, 0, 0, 0x18];
    out.extend_from_slice(b"ftyp");
    out.extend_from_slice(b"isomiso2avc1mp41");
    if len > out.len() {
        out.resize(len, 0xAB);
    }
    out
}

/// A first frame as RGBA, for the core to turn into the ThumbHash.
fn frame(width: u32, height: u32) -> Vec<u8> {
    (0..(width * height))
        .flat_map(|i| {
            let v = (i % 255) as u8;
            [v, 255 - v, 64, 255]
        })
        .collect()
}

async fn wait_video(device: &mut Device) -> ChatMessage {
    let event = device
        .wait_event("a video message", |e| {
            matches!(e, CoreEvent::Message { message }
                if matches!(message.body, ChatBody::Video { .. }))
        })
        .await;
    match event {
        CoreEvent::Message { message } => message,
        _ => unreachable!(),
    }
}

fn video_of(message: &ChatMessage) -> &VideoView {
    match &message.body {
        ChatBody::Video { video } => video,
        other => panic!("expected a video message, got {other:?}"),
    }
}

async fn alice_and_bob(server: &TestServer) -> (Device, Device) {
    let admin = device::admin_with_group(server).await;
    let alice = device::member_of_group(&admin, "alice").await;
    let bob = device::member_of_group(&admin, "bob").await;
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
    (alice, bob)
}

async fn send_clip(who: &Device, len: usize) -> Result<i64, bridge::api::error::CoreError> {
    who.core
        .send_video(mp4(len), 12_000, 854, 480, frame(16, 16), 16, 16)
        .await
}

// spec: app-ui/send-a-video
// spec: app-ui/video-bubble-and-player
// spec: client-runtime/video-normalization-in-a-native-plugin
#[tokio::test(flavor = "multi_thread")]
async fn a_normalized_clip_goes_in_and_a_clip_path_comes_out() {
    let server = TestServer::start().await;
    let (mut alice, mut bob) = alice_and_bob(&server).await;

    let id = send_clip(&alice, 150_000).await.check("send_video");
    alice
        .wait_event("accepted", |e| {
            matches!(e, CoreEvent::SendStatus { id: i, status: SendState::Accepted } if *i == id)
        })
        .await;

    // Bob sees the dimensions, duration and first-frame blur, nothing downloaded.
    let received = wait_video(&mut bob).await;
    let video = video_of(&received);
    assert_eq!((video.width, video.height), (854, 480));
    assert_eq!(video.duration_ms, 12_000);
    assert!(!video.thumbhash.is_empty(), "a first-frame ThumbHash");
    assert_eq!(video.state, MediaStatus::NotDownloaded);
    assert_eq!(bob.core.cached_video(received.id).await.check("cached"), None);

    // The sender has her own clip; Bob's tap gets a path to an MP4 file, never
    // the bytes, and the key never crosses the bridge.
    let own = alice.core.message(id).check("message").check("some");
    assert_eq!(video_of(&own).state, MediaStatus::Available);
    let path = bob.core.download_video(received.id).await.check("download");
    assert!(path.ends_with(".mp4"), "{path}");
    let bytes = std::fs::read(&path).check("read the clip");
    assert_eq!(&bytes[4..8], b"ftyp");
    assert_eq!(
        bob.core.cached_video(received.id).await.check("cached"),
        Some(path)
    );
    let again = bob.core.message(received.id).check("message").check("some");
    assert_eq!(video_of(&again).state, MediaStatus::Available);
}

// spec: app-ui/send-a-video
#[tokio::test(flavor = "multi_thread")]
async fn a_clip_over_the_limit_or_not_an_mp4_has_its_own_errors() {
    let server = TestServer::start().await;
    let (alice, _bob) = alice_and_bob(&server).await;

    // One millisecond over the 30-second cap the core enforces.
    let too_long = alice
        .core
        .send_video(mp4(1000), 30_001, 854, 480, vec![], 0, 0)
        .await;
    assert_eq!(
        too_long.err().map(|e| e.kind),
        Some(CoreErrorKind::VideoTooLong)
    );
    let not_video = alice
        .core
        .send_video(b"not a clip".to_vec(), 5_000, 854, 480, vec![], 0, 0)
        .await;
    assert_eq!(
        not_video.err().map(|e| e.kind),
        Some(CoreErrorKind::NotAVideo)
    );
    assert!(server.blobs().is_empty(), "nothing was uploaded");
}

// spec: app-ui/video-bubble-and-player
#[tokio::test(flavor = "multi_thread")]
async fn an_expired_clip_is_its_own_error_and_state_and_the_sender_resends() {
    let server = TestServer::start().await;
    let (alice, mut bob) = alice_and_bob(&server).await;
    let id = send_clip(&alice, 80_000).await.check("send_video");
    let received = wait_video(&mut bob).await;

    server
        .query(|db| db.execute("UPDATE blobs SET expires_at = 1", []))
        .await;
    server.sweep().await;

    let gone = bob.core.download_video(received.id).await;
    assert_eq!(gone.err().map(|e| e.kind), Some(CoreErrorKind::MediaExpired));
    let now = bob.core.message(received.id).check("message").check("some");
    assert_eq!(video_of(&now).state, MediaStatus::Expired);

    // Bob cannot resend what he did not send.
    let not_mine = bob.core.resend_video(received.id).await;
    assert_eq!(
        not_mine.err().map(|e| e.kind),
        Some(CoreErrorKind::NoSuchMessage)
    );
    let again = alice.core.resend_video(id).await.check("resend");
    assert_ne!(again, id);
    let resent = wait_video(&mut bob).await;
    assert!(bob.core.download_video(resent.id).await.is_ok());
}
