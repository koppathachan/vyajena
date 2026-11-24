//! Voice messages through the bridge (change `audio-messages`): Dart's recorded
//! WAV reaches the core, and the message comes back with its duration, waveform
//! and where the clip is, with no key and no blob id in what Dart holds; the
//! player is handed a file path, never the bytes.

mod support;

use bridge::api::error::CoreErrorKind;
use bridge::api::types::{AudioView, ChatBody, ChatMessage, CoreEvent, MediaStatus, SendState};
use support::Check;
use support::device::{self, Device, user};
use support::server::TestServer;

/// A recorder's WAV: `ms` of a tone, mono, 48 kHz, 16 bit.
fn recording(ms: u64) -> Vec<u8> {
    let rate = 48_000u32;
    let mut data = Vec::new();
    for i in 0..ms * u64::from(rate) / 1000 {
        let t = i as f32 / rate as f32;
        let swell = (0.5 + 0.5 * (2.0 * std::f32::consts::PI * 1.0 * t).sin()).powi(2);
        let tone = (2.0 * std::f32::consts::PI * 180.0 * t).sin();
        data.extend_from_slice(&((0.5 * swell * tone * 32767.0) as i16).to_le_bytes());
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

async fn wait_audio(device: &mut Device) -> ChatMessage {
    let event = device
        .wait_event("a voice message", |e| {
            matches!(e, CoreEvent::Message { message }
                if matches!(message.body, ChatBody::Audio { .. }))
        })
        .await;
    match event {
        CoreEvent::Message { message } => message,
        _ => unreachable!(),
    }
}

fn audio_of(message: &ChatMessage) -> &AudioView {
    match &message.body {
        ChatBody::Audio { audio } => audio,
        other => panic!("expected a voice message, got {other:?}"),
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

// spec: app-ui/record-a-voice-message
// spec: app-ui/play-a-voice-message
#[tokio::test(flavor = "multi_thread")]
async fn a_recording_goes_in_as_wav_and_a_clip_path_comes_out() {
    let server = TestServer::start().await;
    let (mut alice, mut bob) = alice_and_bob(&server).await;

    let id = alice
        .core
        .send_audio(recording(6_000), None)
        .await
        .check("send_audio");
    alice
        .wait_event("accepted", |e| {
            matches!(e, CoreEvent::SendStatus { id: i, status: SendState::Accepted } if *i == id)
        })
        .await;

    // Bob sees the duration and waveform with nothing downloaded.
    let received = wait_audio(&mut bob).await;
    let audio = audio_of(&received);
    assert_eq!(audio.duration_ms, 6_000);
    assert_eq!(audio.waveform.len(), 64);
    assert_eq!(audio.state, MediaStatus::NotDownloaded);
    assert_eq!(
        bob.core.cached_audio(received.id).await.check("cached"),
        None
    );

    // The sender has her own clip; Bob's tap gets a path to an OGG/Opus file.
    let own = alice.core.message(id).check("message").check("some");
    assert_eq!(audio_of(&own).state, MediaStatus::Available);
    let path = bob.core.download_audio(received.id).await.check("download");
    assert!(path.ends_with(".opus"), "{path}");
    let bytes = std::fs::read(&path).check("read the clip");
    assert_eq!(&bytes[..4], b"OggS");
    assert_eq!(
        bob.core.cached_audio(received.id).await.check("cached"),
        Some(path)
    );
    let again = bob.core.message(received.id).check("message").check("some");
    assert_eq!(audio_of(&again).state, MediaStatus::Available);
}

// spec: app-ui/record-a-voice-message
#[tokio::test(flavor = "multi_thread")]
async fn a_recording_that_is_too_long_or_not_audio_has_its_own_errors() {
    let server = TestServer::start().await;
    let (alice, _bob) = alice_and_bob(&server).await;

    let too_long = alice.core.send_audio(recording(125_000), None).await;
    assert_eq!(
        too_long.err().map(|e| e.kind),
        Some(CoreErrorKind::AudioTooLong)
    );
    let not_audio = alice
        .core
        .send_audio(b"not a recording".to_vec(), None)
        .await;
    assert_eq!(
        not_audio.err().map(|e| e.kind),
        Some(CoreErrorKind::AudioUnsupported)
    );
    assert!(server.blobs().is_empty(), "nothing was uploaded");
}

// spec: app-ui/play-a-voice-message
#[tokio::test(flavor = "multi_thread")]
async fn an_expired_clip_is_its_own_error_and_state_and_the_sender_resends() {
    let server = TestServer::start().await;
    let (alice, mut bob) = alice_and_bob(&server).await;
    let id = alice
        .core
        .send_audio(recording(2_000), None)
        .await
        .check("send_audio");
    let received = wait_audio(&mut bob).await;

    server
        .query(|db| db.execute("UPDATE blobs SET expires_at = 1", []))
        .await;
    server.sweep().await;

    let gone = bob.core.download_audio(received.id).await;
    assert_eq!(
        gone.err().map(|e| e.kind),
        Some(CoreErrorKind::MediaExpired)
    );
    let now = bob.core.message(received.id).check("message").check("some");
    assert_eq!(audio_of(&now).state, MediaStatus::Expired);

    // Bob cannot resend what he did not send.
    let not_mine = bob.core.resend_audio(received.id).await;
    assert_eq!(
        not_mine.err().map(|e| e.kind),
        Some(CoreErrorKind::NoSuchMessage)
    );
    let again = alice.core.resend_audio(id).await.check("resend");
    assert_ne!(again, id);
    let resent = wait_audio(&mut bob).await;
    assert!(bob.core.download_audio(resent.id).await.is_ok());
}
