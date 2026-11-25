//! Video messages inside the encrypted `Content` (change `video-messages`):
//! encoding, the checks on both ends, and how a client that predates `Video`
//! reads one (design D1).

use std::time::{Duration, SystemTime};

use client::messaging::{self, Decoded, VideoContent, encode_video};
use prost::Message as _;
use protocol::blob_store::MAX_THUMBHASH_BYTES;
use protocol::video::{MAX_VIDEO_BYTES, MAX_VIDEO_DURATION_MS};

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000)
}

fn video() -> VideoContent {
    VideoContent {
        blob_id: [1; 16],
        blob_key: [2; 32],
        blob_size: 4_000_000,
        duration_ms: 12_000,
        width: 854,
        height: 480,
        thumbhash: vec![9; 25],
    }
}

fn video_of(content: &[u8]) -> VideoContent {
    match messaging::decode(content).body {
        Decoded::Video(video) => video,
        other => panic!("expected a video message, got {other:?}"),
    }
}

/// `vyajena.v1.Video` as it is on the wire, so a test can write one that no
/// well-behaved core would.
#[derive(Clone, PartialEq, prost::Message)]
struct RawVideo {
    #[prost(bytes = "vec", tag = "1")]
    blob_id: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    blob_key: Vec<u8>,
    #[prost(uint64, tag = "3")]
    blob_size: u64,
    #[prost(uint32, tag = "4")]
    duration_ms: u32,
    #[prost(uint32, tag = "5")]
    width: u32,
    #[prost(uint32, tag = "6")]
    height: u32,
    #[prost(bytes = "vec", tag = "7")]
    thumbhash: Vec<u8>,
}

/// A `Content` carrying a `Video` at field 8.
#[derive(Clone, PartialEq, prost::Message)]
struct RawContent {
    #[prost(message, optional, tag = "8")]
    video: Option<RawVideo>,
}

fn good_raw() -> RawVideo {
    RawVideo {
        blob_id: vec![1; 16],
        blob_key: vec![2; 32],
        blob_size: 1_000_000,
        duration_ms: 5_000,
        width: 640,
        height: 360,
        thumbhash: vec![9; 25],
    }
}

fn decoded(raw: RawVideo) -> Decoded {
    let content = RawContent { video: Some(raw) }.encode_to_vec();
    messaging::decode(&content).body
}

/// The shape of `Content` before `Video`: text at 2, nothing at 8.
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

// spec: video-messages/video-messages
// spec: message-content/inner-content-format
#[test]
fn a_video_reference_round_trips() {
    let content = encode_video(&video(), now()).expect("encode");
    assert_eq!(video_of(&content), video());
    assert_eq!(messaging::decode(&content).sent_at, Some(now()));
}

// spec: video-messages/video-messages
#[test]
fn a_video_reference_is_small_next_to_the_ciphertext_cap() {
    // The body carries no clip bytes, only ids, sizes and a tiny ThumbHash.
    let content = encode_video(&video(), now()).expect("encode");
    assert!(content.len() < 250, "{} bytes", content.len());
}

// spec: video-messages/video-messages
#[test]
fn a_malformed_reference_is_shown_as_unreadable_never_dropped() {
    let cases: Vec<(&str, RawVideo)> = vec![
        (
            "a short blob id",
            RawVideo {
                blob_id: vec![1; 15],
                ..good_raw()
            },
        ),
        (
            "a short key",
            RawVideo {
                blob_key: vec![2; 31],
                ..good_raw()
            },
        ),
        (
            "no blob",
            RawVideo {
                blob_size: 0,
                ..good_raw()
            },
        ),
        (
            "a blob over the ceiling",
            RawVideo {
                blob_size: MAX_VIDEO_BYTES as u64 + 1,
                ..good_raw()
            },
        ),
        (
            "no duration",
            RawVideo {
                duration_ms: 0,
                ..good_raw()
            },
        ),
        (
            "a clip over thirty seconds",
            RawVideo {
                duration_ms: MAX_VIDEO_DURATION_MS + 1,
                ..good_raw()
            },
        ),
        (
            "no width",
            RawVideo {
                width: 0,
                ..good_raw()
            },
        ),
        (
            "no height",
            RawVideo {
                height: 0,
                ..good_raw()
            },
        ),
        (
            "a thumbhash over the cap",
            RawVideo {
                thumbhash: vec![1; MAX_THUMBHASH_BYTES + 1],
                ..good_raw()
            },
        ),
    ];
    for (name, raw) in cases {
        assert_eq!(decoded(raw), Decoded::Unsupported, "{name}");
    }

    // The limits themselves are fine, and so is no thumbhash at all.
    for raw in [
        RawVideo {
            duration_ms: MAX_VIDEO_DURATION_MS,
            blob_size: MAX_VIDEO_BYTES as u64,
            thumbhash: vec![0; MAX_THUMBHASH_BYTES],
            ..good_raw()
        },
        RawVideo {
            duration_ms: 1,
            thumbhash: Vec::new(),
            ..good_raw()
        },
    ] {
        assert!(matches!(decoded(raw), Decoded::Video(_)));
    }
}

// spec: video-messages/video-messages
// spec: message-content/inner-content-format
#[test]
fn a_client_that_predates_video_sees_a_body_it_does_not_know() {
    let content = encode_video(&video(), now()).expect("encode");
    let old = OldContent::decode(content.as_slice()).expect("an old core decodes it");
    assert_eq!(
        old.text, None,
        "the old core finds no text and shows its placeholder"
    );
}
