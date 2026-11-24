//! Mentions and replies inside the encrypted `Content` (change `mentions`):
//! encoding, the span checks on both ends, the quote limit on receipt, and
//! how a client that predates the new fields reads them (design D8).

use std::time::{Duration, SystemTime};

use client::messaging::{
    self, ContentError, Decoded, MAX_CONTENT_BYTES, ReplyTo, TextContent, encode_message,
};
use client::{Mention, MentionTarget, UserId};
use prost::Message as _;

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000)
}

fn bob() -> UserId {
    UserId::new("bob", "chat")
}

fn mention(target: MentionTarget, start: usize, len: usize) -> Mention {
    Mention { target, start, len }
}

fn text_of(content: &[u8]) -> TextContent {
    match messaging::decode(content).body {
        Decoded::Text(text) => text,
        other => panic!("expected a text, got {other:?}"),
    }
}

/// The 1.0.0 shape of `Content` and `Text`: no mentions, no reply_to.
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

fn reply_to_alice(quote: &str) -> ReplyTo {
    ReplyTo {
        user: UserId::new("alice", "chat"),
        device: [7; 16],
        msg_id: [9; 16],
        quote: quote.to_owned(),
        original_kind: client::ReplyOriginalKind::Text,
        original_duration_ms: 0,
        original_thumbhash: Vec::new(),
    }
}

// spec: message-content/mentions
#[test]
fn a_mention_of_bob_round_trips() {
    let mentions = [mention(MentionTarget::User(bob()), 3, 4)];
    let content = encode_message("hi @Bob", &mentions, None, now()).expect("encode");
    let text = text_of(&content);
    assert_eq!(text.text, "hi @Bob");
    assert_eq!(text.mentions, mentions);
    assert!(text.mentions[0].names(&bob()));
    assert!(!text.mentions[0].names(&UserId::new("carol", "chat")));
}

// spec: message-content/mentions
#[test]
fn everyone_names_every_user() {
    let mentions = [mention(MentionTarget::Everyone, 0, 9)];
    let content = encode_message("@everyone dinner", &mentions, None, now()).expect("encode");
    let text = text_of(&content);
    assert_eq!(text.mentions, mentions);
    assert!(text.mentions[0].names(&bob()));
    assert!(text.mentions[0].names(&UserId::new("carol", "chat")));
}

// spec: message-content/mentions
#[test]
fn the_sender_refuses_bad_ranges() {
    let user = || MentionTarget::User(bob());
    for bad in [
        vec![mention(user(), 1, 9)],                        // outside the text
        vec![mention(user(), 0, 0)],                        // empty
        vec![mention(user(), 1, 1)],                        // splits "é"
        vec![mention(user(), 0, 2), mention(user(), 1, 2)], // overlap
        vec![mention(user(), usize::MAX, 2)],               // overflow
    ] {
        assert_eq!(
            encode_message("éab", &bad, None, now()),
            Err(ContentError::InvalidMention),
            "{bad:?}"
        );
    }
    // Back to back is fine, in any order.
    let ok = [mention(user(), 2, 1), mention(user(), 0, 2)];
    let text = text_of(&encode_message("éab", &ok, None, now()).expect("encode"));
    assert_eq!(text.mentions, [ok[1].clone(), ok[0].clone()]);
}

// spec: message-content/mentions
#[test]
fn the_receiver_drops_bad_ranges_and_keeps_the_text() {
    // A "hi" with a mention over bytes 1..9, as another client might send.
    let content = client_pb_text("hi", &[(1, 8), (0, 1)]);
    let text = text_of(&content);
    assert_eq!(text.text, "hi");
    assert_eq!(text.mentions, [mention(MentionTarget::User(bob()), 0, 1)]);
}

/// A `Content` with a `Text` and mentions of bob over the given byte ranges,
/// built field by field so the ranges are not checked.
fn client_pb_text(text: &str, ranges: &[(u32, u32)]) -> Vec<u8> {
    #[derive(Clone, PartialEq, prost::Message)]
    struct Content {
        #[prost(message, optional, tag = "2")]
        text: Option<Text>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    struct Text {
        #[prost(string, tag = "1")]
        text: String,
        #[prost(message, repeated, tag = "2")]
        mentions: Vec<PbMention>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    struct PbMention {
        #[prost(message, optional, tag = "1")]
        user: Option<PbUser>,
        #[prost(uint32, tag = "3")]
        start: u32,
        #[prost(uint32, tag = "4")]
        length: u32,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    struct PbUser {
        #[prost(string, tag = "1")]
        username: String,
        #[prost(string, tag = "2")]
        server_id: String,
    }
    Content {
        text: Some(Text {
            text: text.to_owned(),
            mentions: ranges
                .iter()
                .map(|&(start, length)| PbMention {
                    user: Some(PbUser {
                        username: "bob".into(),
                        server_id: "chat".into(),
                    }),
                    start,
                    length,
                })
                .collect(),
        }),
    }
    .encode_to_vec()
}

// spec: message-content/mentions
// spec: message-content/replies
#[test]
fn an_old_client_reads_mentions_and_replies_as_plain_text() {
    let mentions = [mention(MentionTarget::User(bob()), 3, 4)];
    let reply = reply_to_alice("dinner at 8?");
    let content = encode_message("hi @Bob", &mentions, Some(&reply), now()).expect("encode");
    let old = OldContent::decode(content.as_slice()).expect("an old client decodes it");
    assert_eq!(old.text.map(|t| t.text).as_deref(), Some("hi @Bob"));
}

// spec: message-content/replies
#[test]
fn a_reply_carries_the_original_and_its_quote() {
    let reply = reply_to_alice("dinner\n at 8?");
    let content = encode_message("yes", &[], Some(&reply), now()).expect("encode");
    let text = text_of(&content);
    assert_eq!(text.text, "yes");
    assert_eq!(
        text.reply,
        Some(ReplyTo {
            quote: "dinner at 8?".into(),
            ..reply
        })
    );
}

// spec: message-content/replies
#[test]
fn a_long_quote_is_cut_on_both_ends() {
    let long = "x".repeat(300);
    let content = encode_message("yes", &[], Some(&reply_to_alice(&long)), now()).expect("encode");
    let quote = text_of(&content).reply.expect("reply").quote;
    assert_eq!(quote, format!("{}…", "x".repeat(99)));

    // A sender that ignores the limit: the receiver cuts it.
    let raw = raw_reply(&long);
    let quote = text_of(&raw).reply.expect("reply").quote;
    assert_eq!(quote.chars().count(), 100);
}

/// A `Content` with a `Text` "yes" answering Alice with `quote`, built field
/// by field so the quote is not cut.
fn raw_reply(quote: &str) -> Vec<u8> {
    #[derive(Clone, PartialEq, prost::Message)]
    struct Content {
        #[prost(message, optional, tag = "2")]
        text: Option<Text>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    struct Text {
        #[prost(string, tag = "1")]
        text: String,
        #[prost(message, optional, tag = "3")]
        reply_to: Option<Reply>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    struct Reply {
        #[prost(message, optional, tag = "1")]
        sender: Option<Device>,
        #[prost(bytes = "vec", tag = "2")]
        msg_id: Vec<u8>,
        #[prost(string, tag = "3")]
        quote: String,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    struct Device {
        #[prost(message, optional, tag = "1")]
        user_id: Option<User>,
        #[prost(bytes = "vec", tag = "2")]
        device_id: Vec<u8>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    struct User {
        #[prost(string, tag = "1")]
        username: String,
        #[prost(string, tag = "2")]
        server_id: String,
    }
    Content {
        text: Some(Text {
            text: "yes".into(),
            reply_to: Some(Reply {
                sender: Some(Device {
                    user_id: Some(User {
                        username: "alice".into(),
                        server_id: "chat".into(),
                    }),
                    device_id: vec![7; 16],
                }),
                msg_id: vec![9; 16],
                quote: quote.to_owned(),
            }),
        }),
    }
    .encode_to_vec()
}

// spec: message-content/text-only
#[test]
fn mentions_and_the_reply_count_toward_the_size_limit() {
    // The longest text that fits alone no longer fits with a reply.
    let mut text = "a".repeat(MAX_CONTENT_BYTES);
    while messaging::encode_text(&text, now()).is_err() {
        text.pop();
    }
    let reply = reply_to_alice("dinner at 8?");
    assert_eq!(
        encode_message(&text, &[], Some(&reply), now()),
        Err(ContentError::TooLong)
    );
    // And a mention plus a reply is still one Text body.
    let content = encode_message(
        "@Bob yes",
        &[mention(MentionTarget::User(bob()), 0, 4)],
        Some(&reply),
        now(),
    )
    .expect("encode");
    assert!(matches!(messaging::decode(&content).body, Decoded::Text(_)));
}
