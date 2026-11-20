//! Image messages inside the encrypted `Content` (change `media-blobs`):
//! encoding, the checks on both ends, caption mentions, and how a client that
//! predates `Image` reads one (design D1, D11).

use std::time::{Duration, SystemTime};

use client::messaging::{
    self, ContentError, Decoded, ImageContent, MAX_CONTENT_BYTES, encode_image, encode_text,
};
use client::{Mention, MentionTarget, UserId};
use prost::Message as _;
use protocol::blob_store::{MAX_BLOB_BYTES, MAX_IMAGE_EDGE, MAX_THUMBHASH_BYTES};

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000)
}

fn bob() -> UserId {
    UserId::new("bob", "chat")
}

fn image() -> ImageContent {
    ImageContent {
        blob_id: [1; 16],
        blob_key: [2; 32],
        width: 1536,
        height: 2048,
        thumbhash: vec![5; 25],
        blob_size: 181_420,
        caption: None,
        mentions: Vec::new(),
    }
}

fn image_of(content: &[u8]) -> ImageContent {
    match messaging::decode(content).body {
        Decoded::Image(image) => image,
        other => panic!("expected an image, got {other:?}"),
    }
}

/// `vyajena.v1.Image` as it is on the wire, so a test can write one that no
/// well-behaved core would.
#[derive(Clone, PartialEq, prost::Message)]
struct RawImage {
    #[prost(bytes = "vec", tag = "1")]
    blob_id: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    blob_key: Vec<u8>,
    #[prost(uint32, tag = "3")]
    width: u32,
    #[prost(uint32, tag = "4")]
    height: u32,
    #[prost(bytes = "vec", tag = "5")]
    thumbhash: Vec<u8>,
    #[prost(uint64, tag = "6")]
    blob_size: u64,
    #[prost(string, optional, tag = "7")]
    caption: Option<String>,
    #[prost(message, repeated, tag = "8")]
    caption_mentions: Vec<RawMention>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct RawMention {
    #[prost(uint32, tag = "3")]
    start: u32,
    #[prost(uint32, tag = "4")]
    length: u32,
    #[prost(message, optional, tag = "2")]
    everyone: Option<RawEveryone>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct RawEveryone {}

#[derive(Clone, PartialEq, prost::Message)]
struct RawContent {
    #[prost(message, optional, tag = "5")]
    image: Option<RawImage>,
}

fn good_raw() -> RawImage {
    RawImage {
        blob_id: vec![1; 16],
        blob_key: vec![2; 32],
        width: 100,
        height: 80,
        thumbhash: vec![5; 25],
        blob_size: 1000,
        caption: None,
        caption_mentions: Vec::new(),
    }
}

fn decoded(raw: RawImage) -> Decoded {
    let content = RawContent { image: Some(raw) }.encode_to_vec();
    messaging::decode(&content).body
}

/// The 1.1.x shape of `Content`: no `Image` body.
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

// spec: message-content/image-messages
#[test]
fn an_image_reference_round_trips() {
    let content = encode_image(&image(), now()).expect("encode");
    assert_eq!(image_of(&content), image());
    assert_eq!(messaging::decode(&content).sent_at, Some(now()));
}

// spec: message-content/image-messages
#[test]
fn an_image_message_is_small_next_to_the_ciphertext_cap() {
    let content = encode_image(&image(), now()).expect("encode");
    assert!(content.len() < 200, "{} bytes", content.len());
}

// spec: message-content/image-messages
#[test]
fn a_malformed_reference_is_shown_as_unreadable_never_dropped() {
    let edge = MAX_IMAGE_EDGE;
    let cases: Vec<(&str, RawImage)> = vec![
        (
            "a short blob id",
            RawImage {
                blob_id: vec![1; 15],
                ..good_raw()
            },
        ),
        (
            "a short key",
            RawImage {
                blob_key: vec![2; 31],
                ..good_raw()
            },
        ),
        (
            "a long key",
            RawImage {
                blob_key: vec![2; 33],
                ..good_raw()
            },
        ),
        (
            "no width",
            RawImage {
                width: 0,
                ..good_raw()
            },
        ),
        (
            "no height",
            RawImage {
                height: 0,
                ..good_raw()
            },
        ),
        (
            "a width over the limit",
            RawImage {
                width: edge + 1,
                ..good_raw()
            },
        ),
        (
            "a height over the limit",
            RawImage {
                height: edge + 1,
                ..good_raw()
            },
        ),
        (
            "a ThumbHash over the limit",
            RawImage {
                thumbhash: vec![0; MAX_THUMBHASH_BYTES + 1],
                ..good_raw()
            },
        ),
        (
            "an empty blob",
            RawImage {
                blob_size: 0,
                ..good_raw()
            },
        ),
        (
            "a blob over the limit",
            RawImage {
                blob_size: MAX_BLOB_BYTES as u64 + 1,
                ..good_raw()
            },
        ),
    ];
    for (what, raw) in cases {
        assert_eq!(decoded(raw), Decoded::Unsupported, "{what}");
    }
    assert!(matches!(decoded(good_raw()), Decoded::Image(_)));
    // The limits themselves are fine.
    let at_limits = RawImage {
        width: edge,
        height: edge,
        thumbhash: vec![0; MAX_THUMBHASH_BYTES],
        blob_size: MAX_BLOB_BYTES as u64,
        ..good_raw()
    };
    assert!(matches!(decoded(at_limits), Decoded::Image(_)));
}

// spec: message-content/image-caption-and-mentions
#[test]
fn a_caption_with_a_mention_round_trips() {
    let mut with_caption = image();
    with_caption.caption = Some("@Bob look".to_owned());
    with_caption.mentions = vec![Mention {
        target: MentionTarget::User(bob()),
        start: 0,
        len: 4,
    }];
    let content = encode_image(&with_caption, now()).expect("encode");
    assert_eq!(image_of(&content), with_caption);
}

// spec: message-content/image-caption-and-mentions
#[test]
fn the_sender_refuses_a_bad_caption_mention() {
    for (caption, start, len) in [
        (Some("hi"), 1, 8), // outside the text
        (Some("hi"), 0, 0), // empty
        (Some("é"), 1, 1),  // inside a character
        (None, 0, 1),       // a mention without a caption
    ] {
        let mut bad = image();
        bad.caption = caption.map(str::to_owned);
        bad.mentions = vec![Mention {
            target: MentionTarget::Everyone,
            start,
            len,
        }];
        assert_eq!(
            encode_image(&bad, now()),
            Err(ContentError::InvalidMention),
            "{caption:?} {start}..+{len}"
        );
    }
}

// spec: message-content/image-caption-and-mentions
#[test]
fn the_receiver_drops_a_bad_caption_span_and_keeps_the_caption() {
    let raw = RawImage {
        caption: Some("hi".to_owned()),
        caption_mentions: vec![RawMention {
            start: 1,
            length: 8,
            everyone: Some(RawEveryone {}),
        }],
        ..good_raw()
    };
    let Decoded::Image(image) = decoded(raw) else {
        panic!("expected an image");
    };
    assert_eq!(image.caption.as_deref(), Some("hi"));
    assert!(image.mentions.is_empty());
}

// spec: message-content/image-caption-and-mentions
#[test]
fn a_caption_is_limited_like_a_text_message() {
    // The same cap as `Text`: what encrypts within the 65,536-byte ciphertext.
    let mut long = image();
    long.caption = Some("a".repeat(MAX_CONTENT_BYTES));
    assert_eq!(encode_image(&long, now()), Err(ContentError::TooLong));
    assert_eq!(
        encode_text(&"a".repeat(MAX_CONTENT_BYTES), now()),
        Err(ContentError::TooLong)
    );
    long.caption = Some("a".repeat(MAX_CONTENT_BYTES - 300));
    assert!(encode_image(&long, now()).is_ok());
}

// spec: message-content/image-messages
#[test]
fn a_client_that_predates_images_sees_a_body_it_does_not_know() {
    let content = encode_image(&image(), now()).expect("encode");
    let old = OldContent::decode(content.as_slice()).expect("an old core decodes it");
    assert_eq!(
        old.text, None,
        "the old core finds no text and shows its placeholder"
    );
}
