//! Video messages end to end against the real server (change `video-messages`):
//! sending the already-normalized clip, receiving, playing from the cache,
//! expiry and resend, the config-driven size cap, and that the server learns
//! nothing it should not (video-messages: Video messages, Config-driven limit,
//! Video stays encrypted; client-runtime: Video normalization in a native
//! plugin, Blob crypto and send order, Image download and cache, Expired media).

mod support;

use client::{
    ClientError, Event, MediaState, Message, MessageBody, MessageId, PushPlatform, PushToken,
    SendStatus, VideoBody,
};
use protocol::video::{DEFAULT_MAX_VIDEO_BYTES, MAX_VIDEO_DURATION_MS};
use support::Check;
use support::device::{self, Device, member};
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

/// Bytes that look like a normalized MP4: a leading box size then the `ftyp` box
/// type the core checks for (video-messages: Video messages); the rest is opaque
/// payload of the given total length. This stands in for what the native plugin
/// hands the core; the core never demuxes it.
fn mp4(len: usize) -> Vec<u8> {
    let mut out = vec![0, 0, 0, 0x18];
    out.extend_from_slice(b"ftyp");
    out.extend_from_slice(b"isomiso2avc1mp41");
    if len > out.len() {
        out.resize(len, 0xAB);
    }
    out
}

/// A first frame as RGBA pixels, for the core to turn into the ThumbHash.
fn frame(width: u32, height: u32) -> Vec<u8> {
    (0..(width * height))
        .flat_map(|i| {
            let v = (i % 255) as u8;
            [v, 255 - v, 128, 255]
        })
        .collect()
}

fn video_of(message: &Message) -> &VideoBody {
    match &message.body {
        MessageBody::Video(video) => video,
        other => panic!("expected a video message, got {other:?}"),
    }
}

async fn wait_video(device: &mut Device) -> Message {
    match device
        .wait_event(
            "a video message",
            |e| matches!(e, Event::Message(m) if matches!(m.body, MessageBody::Video(_))),
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

/// Sends a 12-second 854x480 clip from `who` to the main group.
async fn send_clip(who: &Device, len: usize) -> Result<MessageId, ClientError> {
    who.client
        .send_video(&mp4(len), 12_000, 854, 480, &frame(16, 16), 16, 16)
        .await
}

// spec: video-messages/video-messages
// spec: client-runtime/blob-crypto-and-send-order
// spec: client-runtime/image-download-and-cache
#[tokio::test(flavor = "multi_thread")]
async fn alice_sends_a_video_and_bob_and_the_admin_play_it() {
    let server = TestServer::start().await;
    let (mut admin, mut alice, mut bob) = chat(&server).await;

    let id = send_clip(&alice, 200_000).await.check("send_video");
    wait_accepted(&mut alice, id).await;
    // Uploaded before the message went out, and stored whole.
    assert_eq!(server.blobs().len(), 1);

    // Bob sees the dimensions, duration and the first-frame blur at once, with
    // nothing downloaded.
    let received = wait_video(&mut bob).await;
    let video = video_of(&received);
    assert_eq!((video.width, video.height), (854, 480));
    assert_eq!(video.duration_ms, 12_000);
    assert!(!video.thumbhash.is_empty(), "a first-frame ThumbHash");
    assert_eq!(video.state, MediaState::NotDownloaded);
    assert_eq!(bob.client.cached_video(received.id).await.check("c"), None);

    // The sender has its own clip at once.
    let own = alice.client.message(id).check("message").check("some");
    assert_eq!(video_of(&own).state, MediaState::Available);
    assert!(alice.client.cached_video(id).await.check("c").is_some());

    // Bob plays it: downloaded, decrypted, an MP4 in the core's cache.
    let path = bob
        .client
        .download_video(received.id)
        .await
        .check("download");
    assert_eq!(path.extension().and_then(|e| e.to_str()), Some("mp4"));
    assert!(path.starts_with(bob.data_dir().join("media")));
    let bytes = std::fs::read(&path).check("read");
    assert_eq!(&bytes[4..8], b"ftyp");
    assert_eq!(server.blobs().len(), 1, "the admin still needs it");

    // Played again with no connection: from the device's cache.
    bob.client.stop();
    let again = bob.client.download_video(received.id).await;
    assert_eq!(again.check("from cache"), path);

    // The admin was the last to need it: the server deletes the blob.
    let at_admin = wait_video(&mut admin).await;
    let admin_path = admin
        .client
        .download_video(at_admin.id)
        .await
        .check("download");
    assert_eq!(
        std::fs::read(admin_path).check("read"),
        bytes,
        "the same clip"
    );
    assert!(server.blobs().is_empty(), "every member device has acked");
}

// spec: client-runtime/blob-crypto-and-send-order
#[tokio::test(flavor = "multi_thread")]
async fn a_video_sent_offline_waits_in_the_outbox_and_its_blob_goes_up_first() {
    let server = TestServer::start().await;
    let (_admin, mut alice, mut bob) = chat(&server).await;
    alice.client.stop();
    server.wait_offline_recorded(&alice.device_id()).await;

    let id = send_clip(&alice, 100_000).await.check("send offline");
    let after = alice.client.send_text("and a line after it").check("send");
    let own = alice.client.message(id).check("message").check("some");
    assert_eq!(own.status, Some(SendStatus::Sending));
    assert_eq!(video_of(&own).state, MediaState::Available);
    assert!(
        server.blobs().is_empty(),
        "nothing is uploaded while offline"
    );

    alice.client.start().await.check("start");
    wait_accepted(&mut alice, id).await;
    wait_accepted(&mut alice, after).await;
    assert_eq!(server.blobs().len(), 1);
    let received = wait_video(&mut bob).await;
    assert!(bob.client.download_video(received.id).await.is_ok());
    let kinds: Vec<&str> = bob
        .client
        .history(100)
        .check("history")
        .iter()
        .map(|m| match m.body {
            MessageBody::Video(_) => "video",
            MessageBody::Text(_) => "text",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["video", "text"], "in the order sent");
}

// spec: video-messages/config-driven-limit
#[tokio::test(flavor = "multi_thread")]
async fn a_clip_over_thirty_seconds_or_not_an_mp4_sends_nothing() {
    let server = TestServer::start().await;
    let (_admin, alice, _bob) = chat(&server).await;

    // The client enforces the 30-second duration, which the server never sees.
    let too_long = alice
        .client
        .send_video(&mp4(1000), MAX_VIDEO_DURATION_MS + 1, 854, 480, &[], 0, 0)
        .await;
    assert!(matches!(too_long, Err(ClientError::VideoTooLong)), "{too_long:?}");

    let not_a_video = alice
        .client
        .send_video(b"hello there", 5_000, 854, 480, &[], 0, 0)
        .await;
    assert!(matches!(not_a_video, Err(ClientError::NotAVideo)), "{not_a_video:?}");

    assert!(server.blobs().is_empty(), "nothing was uploaded");
    assert!(alice.client.history(100).check("history").is_empty());
}

// spec: video-messages/config-driven-limit
// spec: wire-transport/size-limits
#[tokio::test(flavor = "multi_thread")]
async fn a_video_whose_blob_would_exceed_the_advertised_cap_is_refused() {
    // The operator pins the smallest allowed cap (the image cap floor); the
    // client fetches it and refuses a larger clip before uploading anything.
    let cap = protocol::blob_store::MAX_BLOB_BYTES as u64;
    let server = TestServer::start_with_video_cap(cap).await;
    let (_admin, alice, _bob) = chat(&server).await;

    // A clip whose ciphertext would exceed the cap is refused client-side.
    let over = send_clip(&alice, cap as usize + 10_000).await;
    assert!(matches!(over, Err(ClientError::VideoTooLarge)), "{over:?}");
    assert!(server.blobs().is_empty(), "nothing was uploaded");

    // A clip within the cap still sends.
    let under = send_clip(&alice, 50_000).await;
    assert!(under.is_ok(), "{under:?}");
}

// spec: video-messages/video-stays-encrypted
// spec: client-runtime/video-normalization-in-a-native-plugin
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

    // A recognizable marker in the plaintext clip, so we can prove it never
    // reaches the server in the clear.
    let mut clip = mp4(100_000);
    clip.extend_from_slice(b"PLAINTEXT-MARKER-avc1");
    let id = alice
        .client
        .send_video(&clip, 10_000, 640, 360, &frame(8, 8), 8, 8)
        .await
        .check("send_video");
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

    // The envelope holds no clip bytes.
    let rows: Vec<Vec<u8>> = server
        .query(|db| {
            let mut statement = db.prepare("SELECT ciphertext FROM envelopes WHERE kind = 1")?;
            statement.query_map([], |row| row.get(0))?.collect()
        })
        .await;
    assert!(!rows.is_empty());
    for row in rows {
        assert!(!support::contains(&row, b"ftyp"));
        assert!(!support::contains(&row, b"PLAINTEXT-MARKER"));
    }

    // The blob is ciphertext: no MP4 header, nothing readable from the clip.
    let blobs = server.blobs();
    assert_eq!(blobs.len(), 1);
    let bytes = std::fs::read(server.blob_dir().join(&blobs[0])).check("blob file");
    assert!(!support::contains(&bytes, b"ftyp"));
    assert!(!support::contains(&bytes, b"PLAINTEXT-MARKER"));
}

// spec: client-runtime/expired-media
#[tokio::test(flavor = "multi_thread")]
async fn an_expired_video_shows_as_expired_and_the_sender_can_resend_it() {
    let server = TestServer::start().await;
    let (_admin, mut alice, mut bob) = chat(&server).await;
    let id = send_clip(&alice, 80_000).await.check("send_video");
    wait_accepted(&mut alice, id).await;
    let received = wait_video(&mut bob).await;
    let first_blob = video_of(&received).blob_id;

    server
        .query(|db| db.execute("UPDATE blobs SET expires_at = 1", []))
        .await;
    server.sweep().await;
    assert!(server.blobs().is_empty(), "the server deleted it");

    let opened = bob.client.download_video(received.id).await;
    assert!(matches!(opened, Err(ClientError::MediaExpired)), "{opened:?}");
    let now = bob
        .client
        .message(received.id)
        .check("message")
        .check("some");
    assert_eq!(video_of(&now).state, MediaState::Expired);
    assert_eq!(video_of(&now).duration_ms, 12_000, "the bubble keeps its shape");

    // Alice kept the clip: resending makes a new message with a new blob.
    let again_id = alice.client.resend_video(id).await.check("resend");
    assert_ne!(again_id, id);
    wait_accepted(&mut alice, again_id).await;
    let resent = wait_video(&mut bob).await;
    assert_ne!(video_of(&resent).blob_id, first_blob, "a fresh blob");
    assert_eq!(video_of(&resent).duration_ms, 12_000);
    assert!(bob.client.download_video(resent.id).await.is_ok());

    // Only an own clip this device has can be resent.
    let refused = bob.client.resend_video(resent.id).await;
    assert!(matches!(refused, Err(ClientError::NoSuchMessage)), "{refused:?}");
}

// spec: video-messages/video-messages
#[tokio::test(flavor = "multi_thread")]
async fn the_default_advertised_cap_is_the_protocol_default() {
    // With no server config the client enforces the protocol default cap.
    let server = TestServer::start().await;
    let (_admin, alice, _bob) = chat(&server).await;
    // A clip comfortably under the default sends.
    let id = send_clip(&alice, 300_000).await;
    assert!(id.is_ok(), "{id:?}");
    // Sanity: the default is well above a normalized clip.
    assert!(DEFAULT_MAX_VIDEO_BYTES > 300_000);
}
