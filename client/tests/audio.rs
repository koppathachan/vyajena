//! Voice messages end to end against the real server (change `audio-messages`):
//! sending, receiving, playing from the cache, expiry and resend, and that the
//! server learns nothing it should not (audio-messages: Audio messages,
//! Recording and encoding, Audio stays encrypted; client-runtime: Blob crypto and
//! send order, Image download and cache, Expired media).

mod support;

use client::media::audio::is_ogg_opus;
use client::{
    AudioBody, ClientError, Event, MediaState, Message, MessageBody, MessageId, PushPlatform,
    PushToken, SendStatus,
};
use protocol::audio::MAX_AUDIO_DURATION_MS;
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

/// A recorder's WAV: `ms` of a swelling tone, mono, 48 kHz, 16 bit.
fn recording(ms: u64) -> Vec<u8> {
    let rate = 48_000u32;
    let mut data = Vec::new();
    for i in 0..ms * u64::from(rate) / 1000 {
        let t = i as f32 / rate as f32;
        let swell = (0.5 + 0.5 * (2.0 * std::f32::consts::PI * 1.2 * t).sin()).powi(2);
        let tone = (2.0 * std::f32::consts::PI * 160.0 * t).sin();
        data.extend_from_slice(&((0.5 * swell * tone * 32767.0) as i16).to_le_bytes());
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    for field in [1u16, 1] {
        out.extend_from_slice(&field.to_le_bytes());
    }
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

fn audio_of(message: &Message) -> &AudioBody {
    match &message.body {
        MessageBody::Audio(audio) => audio,
        other => panic!("expected a voice message, got {other:?}"),
    }
}

async fn wait_audio(device: &mut Device) -> Message {
    match device
        .wait_event(
            "a voice message",
            |e| matches!(e, Event::Message(m) if matches!(m.body, MessageBody::Audio(_))),
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

// spec: audio-messages/audio-messages
// spec: audio-messages/recording-and-encoding
// spec: client-runtime/blob-crypto-and-send-order
// spec: client-runtime/image-download-and-cache
#[tokio::test(flavor = "multi_thread")]
async fn alice_sends_a_voice_message_and_bob_and_the_admin_play_it() {
    let server = TestServer::start().await;
    let (mut admin, mut alice, mut bob) = chat(&server).await;

    let id = alice
        .client
        .send_audio(&recording(20_000), None)
        .await
        .check("send_audio");
    wait_accepted(&mut alice, id).await;
    // Uploaded before the message went out, and stored whole.
    assert_eq!(server.blobs().len(), 1);

    // Bob sees the duration and the waveform at once, with nothing downloaded.
    let received = wait_audio(&mut bob).await;
    let audio = audio_of(&received);
    assert_eq!(audio.duration_ms, 20_000);
    assert_eq!(audio.waveform.len(), 64);
    assert_eq!(audio.waveform.iter().max(), Some(&255));
    assert_eq!(audio.state, MediaState::NotDownloaded);
    assert_eq!(bob.client.cached_audio(received.id).await.check("c"), None);
    assert!(received.mentions.is_empty() && !received.mentions_me);

    // The sender has its own clip at once.
    let own = alice.client.message(id).check("message").check("some");
    assert_eq!(audio_of(&own).state, MediaState::Available);
    assert!(alice.client.cached_audio(id).await.check("c").is_some());

    // Bob plays it: downloaded, decrypted, an OGG/Opus file in the core's cache.
    let path = bob
        .client
        .download_audio(received.id)
        .await
        .check("download");
    assert_eq!(path.extension().and_then(|e| e.to_str()), Some("opus"));
    assert!(path.starts_with(bob.data_dir().join("media")));
    let ogg = std::fs::read(&path).check("read");
    assert!(is_ogg_opus(&ogg));
    let now = bob
        .client
        .message(received.id)
        .check("message")
        .check("some");
    assert_eq!(audio_of(&now).state, MediaState::Available);
    assert_eq!(server.blobs().len(), 1, "the admin still needs it");

    // Played again with no connection: from the device's cache.
    bob.client.stop();
    let again = bob.client.download_audio(received.id).await;
    assert_eq!(again.check("from cache"), path);

    // The admin was the last to need it: the server deletes the blob.
    let at_admin = wait_audio(&mut admin).await;
    let admin_path = admin
        .client
        .download_audio(at_admin.id)
        .await
        .check("download");
    assert_eq!(
        std::fs::read(admin_path).check("read"),
        ogg,
        "the same clip"
    );
    assert!(server.blobs().is_empty(), "every member device has acked");
}

// spec: client-runtime/blob-crypto-and-send-order
#[tokio::test(flavor = "multi_thread")]
async fn a_voice_message_sent_offline_waits_in_the_outbox_and_its_blob_goes_up_first() {
    let server = TestServer::start().await;
    let (_admin, mut alice, mut bob) = chat(&server).await;
    alice.client.stop();
    server.wait_offline_recorded(&alice.device_id()).await;

    let id = alice
        .client
        .send_audio(&recording(3_000), None)
        .await
        .check("send_audio offline");
    let after = alice.client.send_text("and a line after it").check("send");
    let own = alice.client.message(id).check("message").check("some");
    assert_eq!(own.status, Some(SendStatus::Sending));
    assert_eq!(audio_of(&own).state, MediaState::Available);
    assert!(
        server.blobs().is_empty(),
        "nothing is uploaded while offline"
    );

    alice.client.start().await.check("start");
    wait_accepted(&mut alice, id).await;
    wait_accepted(&mut alice, after).await;
    assert_eq!(server.blobs().len(), 1);
    let received = wait_audio(&mut bob).await;
    assert!(bob.client.download_audio(received.id).await.is_ok());
    let kinds: Vec<&str> = bob
        .client
        .history(100)
        .check("history")
        .iter()
        .map(|m| match m.body {
            MessageBody::Audio(_) => "audio",
            MessageBody::Text(_) => "text",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["audio", "text"], "in the order sent");
}

// spec: client-runtime/blob-crypto-and-send-order
#[tokio::test(flavor = "multi_thread")]
async fn a_queued_voice_message_whose_clip_is_gone_fails_and_the_chat_goes_on() {
    let server = TestServer::start().await;
    let (_admin, mut alice, _bob) = chat(&server).await;
    alice.client.stop();
    server.wait_offline_recorded(&alice.device_id()).await;
    let id = alice
        .client
        .send_audio(&recording(2_000), None)
        .await
        .check("send_audio offline");
    std::fs::remove_dir_all(alice.data_dir().join("media")).check("clear the cache");

    alice.client.start().await.check("start");
    let reason = match alice
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
    };
    assert!(
        reason.contains("voice message is no longer on this device"),
        "{reason}"
    );
    assert!(server.blobs().is_empty());
}

// spec: audio-messages/recording-and-encoding
#[tokio::test(flavor = "multi_thread")]
async fn a_recording_that_is_too_long_or_not_audio_sends_nothing() {
    let server = TestServer::start().await;
    let (_admin, alice, _bob) = chat(&server).await;

    let too_long = alice
        .client
        .send_audio(&recording(u64::from(MAX_AUDIO_DURATION_MS) + 5_000), None)
        .await;
    assert!(
        matches!(too_long, Err(ClientError::Media(_))),
        "{too_long:?}"
    );
    let not_audio = alice.client.send_audio(b"hello", None).await;
    assert!(
        matches!(not_audio, Err(ClientError::Media(_))),
        "{not_audio:?}"
    );

    assert!(server.blobs().is_empty(), "nothing was uploaded");
    assert!(alice.client.history(100).check("history").is_empty());
}

// spec: client-runtime/expired-media
#[tokio::test(flavor = "multi_thread")]
async fn an_expired_voice_message_shows_as_expired_and_the_sender_can_resend_it() {
    let server = TestServer::start().await;
    let (_admin, mut alice, mut bob) = chat(&server).await;
    let id = alice
        .client
        .send_audio(&recording(4_000), None)
        .await
        .check("send_audio");
    wait_accepted(&mut alice, id).await;
    let received = wait_audio(&mut bob).await;
    let first_blob = audio_of(&received).blob_id;

    server
        .query(|db| db.execute("UPDATE blobs SET expires_at = 1", []))
        .await;
    server.sweep().await;
    assert!(server.blobs().is_empty(), "the server deleted it");

    let opened = bob.client.download_audio(received.id).await;
    assert!(
        matches!(opened, Err(ClientError::MediaExpired)),
        "{opened:?}"
    );
    let now = bob
        .client
        .message(received.id)
        .check("message")
        .check("some");
    assert_eq!(audio_of(&now).state, MediaState::Expired);
    assert_eq!(
        audio_of(&now).duration_ms,
        4_000,
        "the bubble keeps its shape"
    );

    // Alice kept the clip: resending makes a new message with a new blob.
    let again_id = alice.client.resend_audio(id).await.check("resend");
    assert_ne!(again_id, id);
    wait_accepted(&mut alice, again_id).await;
    let resent = wait_audio(&mut bob).await;
    assert_ne!(audio_of(&resent).blob_id, first_blob, "a fresh blob");
    assert_eq!(audio_of(&resent).duration_ms, 4_000);
    let path = bob.client.download_audio(resent.id).await.check("download");
    assert!(is_ogg_opus(&std::fs::read(path).check("read")));

    // Only an own clip this device has can be resent.
    let refused = bob.client.resend_audio(resent.id).await;
    assert!(
        matches!(refused, Err(ClientError::NoSuchMessage)),
        "{refused:?}"
    );
    let text = alice.client.send_text("hello").check("send");
    let refused = alice.client.resend_audio(text).await;
    assert!(
        matches!(refused, Err(ClientError::NoSuchMessage)),
        "{refused:?}"
    );
}

// spec: audio-messages/audio-stays-encrypted
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
        .send_audio(&recording(5_000), None)
        .await
        .check("send_audio");
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
        assert!(!support::contains(&row, b"OggS"));
        assert!(!support::contains(&row, b"OpusHead"));
    }

    // The blob is ciphertext: no OGG header, no Opus header, nothing readable.
    let blobs = server.blobs();
    assert_eq!(blobs.len(), 1);
    let bytes = std::fs::read(server.blob_dir().join(&blobs[0])).check("blob file");
    assert!(!support::contains(&bytes, b"OggS"));
    assert!(!support::contains(&bytes, b"OpusHead"));
}
