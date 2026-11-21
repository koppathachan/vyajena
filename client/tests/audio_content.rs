//! Voice messages inside the encrypted `Content` (change `audio-messages`):
//! encoding, the checks on both ends, and how a client that predates `Audio`
//! reads one (design D1).

use std::time::{Duration, SystemTime};

use client::messaging::{self, AudioContent, Decoded, encode_audio};
use prost::Message as _;
use protocol::audio::{MAX_AUDIO_DURATION_MS, MAX_AUDIO_WAVEFORM_BYTES};
use protocol::blob_store::MAX_BLOB_BYTES;

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000)
}

fn audio() -> AudioContent {
    AudioContent {
        blob_id: [1; 16],
        blob_key: [2; 32],
        blob_size: 61_000,
        duration_ms: 20_000,
        waveform: (0..64).map(|i| (i * 4) as u8).collect(),
    }
}

fn audio_of(content: &[u8]) -> AudioContent {
    match messaging::decode(content).body {
        Decoded::Audio(audio) => audio,
        other => panic!("expected a voice message, got {other:?}"),
    }
}

/// `vyajena.v1.Audio` as it is on the wire, so a test can write one that no
/// well-behaved core would.
#[derive(Clone, PartialEq, prost::Message)]
struct RawAudio {
    #[prost(bytes = "vec", tag = "1")]
    blob_id: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    blob_key: Vec<u8>,
    #[prost(uint64, tag = "3")]
    blob_size: u64,
    #[prost(uint32, tag = "4")]
    duration_ms: u32,
    #[prost(bytes = "vec", tag = "5")]
    waveform: Vec<u8>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct RawContent {
    #[prost(message, optional, tag = "6")]
    audio: Option<RawAudio>,
}

fn good_raw() -> RawAudio {
    RawAudio {
        blob_id: vec![1; 16],
        blob_key: vec![2; 32],
        blob_size: 1000,
        duration_ms: 5_000,
        waveform: vec![9; 64],
    }
}

fn decoded(raw: RawAudio) -> Decoded {
    let content = RawContent { audio: Some(raw) }.encode_to_vec();
    messaging::decode(&content).body
}

/// The shape of `Content` before `Audio`: text at 2, an image at 5, nothing at 6.
#[derive(Clone, PartialEq, prost::Message)]
struct OldContent {
    #[prost(message, optional, tag = "2")]
    text: Option<OldText>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct OldText {
    #[prost(string, tag = "1")]
    text: String,
}

// spec: audio-messages/audio-messages
// spec: message-content/inner-content-format
#[test]
fn a_voice_reference_round_trips() {
    let content = encode_audio(&audio(), now()).expect("encode");
    assert_eq!(audio_of(&content), audio());
    assert_eq!(messaging::decode(&content).sent_at, Some(now()));
}

// spec: audio-messages/audio-messages
#[test]
fn a_voice_message_is_small_next_to_the_ciphertext_cap() {
    let content = encode_audio(&audio(), now()).expect("encode");
    assert!(content.len() < 250, "{} bytes", content.len());
}

// spec: audio-messages/audio-messages
#[test]
fn a_malformed_reference_is_shown_as_unreadable_never_dropped() {
    let cases: Vec<(&str, RawAudio)> = vec![
        (
            "a short blob id",
            RawAudio {
                blob_id: vec![1; 15],
                ..good_raw()
            },
        ),
        (
            "a short key",
            RawAudio {
                blob_key: vec![2; 31],
                ..good_raw()
            },
        ),
        (
            "no blob",
            RawAudio {
                blob_size: 0,
                ..good_raw()
            },
        ),
        (
            "a blob over the cap",
            RawAudio {
                blob_size: MAX_BLOB_BYTES as u64 + 1,
                ..good_raw()
            },
        ),
        (
            "no duration",
            RawAudio {
                duration_ms: 0,
                ..good_raw()
            },
        ),
        (
            "a clip over two minutes",
            RawAudio {
                duration_ms: MAX_AUDIO_DURATION_MS + 1,
                ..good_raw()
            },
        ),
        (
            "a waveform over the cap",
            RawAudio {
                waveform: vec![1; MAX_AUDIO_WAVEFORM_BYTES + 1],
                ..good_raw()
            },
        ),
    ];
    for (name, raw) in cases {
        assert_eq!(decoded(raw), Decoded::Unsupported, "{name}");
    }

    // The limits themselves are fine, and so is no waveform at all.
    for raw in [
        RawAudio {
            duration_ms: MAX_AUDIO_DURATION_MS,
            blob_size: MAX_BLOB_BYTES as u64,
            waveform: vec![0; MAX_AUDIO_WAVEFORM_BYTES],
            ..good_raw()
        },
        RawAudio {
            duration_ms: 1,
            waveform: Vec::new(),
            ..good_raw()
        },
    ] {
        assert!(matches!(decoded(raw), Decoded::Audio(_)));
    }
}

// spec: audio-messages/audio-messages
// spec: message-content/inner-content-format
#[test]
fn a_client_that_predates_audio_sees_a_body_it_does_not_know() {
    let content = encode_audio(&audio(), now()).expect("encode");
    let old = OldContent::decode(content.as_slice()).expect("an old core decodes it");
    assert_eq!(
        old.text, None,
        "the old core finds no text and shows its placeholder"
    );
}
