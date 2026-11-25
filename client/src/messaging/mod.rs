//! Content bodies, and receiving application messages (client-core task
//! group 6; message-content; client-runtime: Events in order).
//!
//! The plaintext of every application message is an encoded
//! `vyajena.v1.Content` (message-content: Inner content format): a send time
//! and one of `Text`, `Image`, `Audio`, `GroupSettings` or `Profile`. Only this core encodes or
//! decodes it. Sending lives in the engine (`engine/outbox.rs`), which owns
//! the group; [`receive_application`] is the step it runs for each incoming
//! `APPLICATION` envelope inside the store transaction that also records
//! `processed_seq`, so the message is stored before it is acked.

use std::time::SystemTime;

use openmls::prelude::tls_codec::Deserialize as _;
use openmls::prelude::{MlsGroup, MlsMessageIn, ProcessedMessageContent, Sender};
use prost::Message as _;

use crate::engine::EngineError;
use crate::events::{
    DeviceId, MediaState, Mention, MentionTarget, Message, MessageBody, Reply, ReplyOriginalKind,
    SendStatus, UserId,
};
use crate::identity::certificate::{from_timestamp, timestamp};
use crate::media::{BlobId, BlobKey};
use crate::mls::verify;
use crate::pb;
use crate::pb::content::Body;
use crate::store::{IncomingMessage, InsertOutcome, ProfileRow, Store, StoredMessage};

/// The largest encoded `Content` the core sends. An application message's
/// ciphertext is capped at
/// [`protocol::wire_transport::MAX_APPLICATION_CIPHERTEXT_BYTES`]
/// (wire-transport: Size limits); MLS adds a fixed overhead of under 200
/// bytes (headers, the encrypted sender data, the signature and the AEAD
/// tags; no padding). This leaves a 512-byte margin, so any `Content` up to
/// this size encrypts within the cap (message-content: Text only). The
/// engine checks the real ciphertext again before sending.
pub const MAX_CONTENT_BYTES: usize =
    protocol::wire_transport::MAX_APPLICATION_CIPHERTEXT_BYTES - 512;

/// A `Text` body, decoded: the text, its valid mentions and what it answers
/// (message-content: Text only; Mentions; Replies).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextContent {
    /// The text as the sender typed it.
    pub text: String,
    /// The valid mentions, sorted by `start`; invalid ones are dropped.
    pub mentions: Vec<Mention>,
    /// What it answers, if it is a reply with a well-formed reference.
    pub reply: Option<ReplyTo>,
}

/// A `ReplyTo`, decoded (message-content: Replies; Reply quote describes the
/// original's kind).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyTo {
    /// The user who sent the original.
    pub user: UserId,
    /// The device that sent the original.
    pub device: DeviceId,
    /// The original envelope's `msg_id`.
    pub msg_id: [u8; 16],
    /// The quote, cut to `MAX_REPLY_QUOTE_CHARS` (empty for a voice original).
    pub quote: String,
    /// What the original was, so a receiver without it renders the right preview.
    pub original_kind: ReplyOriginalKind,
    /// For a voice original, its length in milliseconds (0 otherwise).
    pub original_duration_ms: u32,
    /// For a picture original, its ThumbHash (empty otherwise), so an image
    /// reply shows a thumbnail without holding the original.
    pub original_thumbhash: Vec<u8>,
}

/// An `Image` body, decoded and checked (message-content: Image messages). It
/// keeps the blob key; the UI's [`crate::ImageBody`] does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageContent {
    /// The blob's id.
    pub blob_id: BlobId,
    /// The blob's key.
    pub blob_key: BlobKey,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// The ThumbHash blur.
    pub thumbhash: Vec<u8>,
    /// The blob's size on the server.
    pub blob_size: u64,
    /// The caption, if any.
    pub caption: Option<String>,
    /// The valid caption mentions, sorted by `start`; empty without a caption.
    pub mentions: Vec<Mention>,
}

/// An `Audio` body, decoded and checked (message-content: Audio messages). It
/// keeps the blob key; the UI's [`crate::AudioBody`] does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioContent {
    /// The blob's id.
    pub blob_id: BlobId,
    /// The blob's key.
    pub blob_key: BlobKey,
    /// The blob's size on the server.
    pub blob_size: u64,
    /// The clip's length in milliseconds.
    pub duration_ms: u32,
    /// The amplitude peaks, one byte each.
    pub waveform: Vec<u8>,
    /// What this voice message answers, if it is a reply (message-content:
    /// Voice replies).
    pub reply: Option<ReplyTo>,
}

/// A `Video` body, decoded and checked (message-content: Video messages). It
/// keeps the blob key; the UI's [`crate::VideoBody`] does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoContent {
    /// The blob's id.
    pub blob_id: BlobId,
    /// The blob's key.
    pub blob_key: BlobKey,
    /// The blob's size on the server.
    pub blob_size: u64,
    /// The clip's length in milliseconds.
    pub duration_ms: u32,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// The first-frame ThumbHash blur.
    pub thumbhash: Vec<u8>,
}

/// What a `Content` says, decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decoded {
    /// A text message.
    Text(TextContent),
    /// An image message.
    Image(ImageContent),
    /// A voice message.
    Audio(AudioContent),
    /// A video message.
    Video(VideoContent),
    /// `GroupSettings`: the group's name.
    GroupSettings {
        /// The name, if set.
        name: Option<String>,
    },
    /// `Profile`: the sender's own profile.
    Profile(ProfileRow),
    /// A body this version does not know (the oneof decodes as unset), or
    /// bytes that do not decode at all: shown as "needs a newer app
    /// version", never an error (message-content: Inner content format).
    Unsupported,
}

/// A decoded `Content`: its send time and body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedContent {
    /// The sender's clock when it was sent.
    pub sent_at: Option<SystemTime>,
    /// The body.
    pub body: Decoded,
}

/// Decodes an encoded `Content`.
pub fn decode(content: &[u8]) -> DecodedContent {
    let Ok(content) = pb::Content::decode(content) else {
        return DecodedContent {
            sent_at: None,
            body: Decoded::Unsupported,
        };
    };
    let body = match content.body {
        Some(Body::Text(text)) => Decoded::Text(decode_text(text)),
        Some(Body::GroupSettings(settings)) => Decoded::GroupSettings {
            name: settings.name,
        },
        Some(Body::Profile(profile)) => Decoded::Profile(ProfileRow {
            display_name: profile.display_name,
            about: profile.about,
        }),
        // An image reference that is not well formed is shown as a message this
        // version cannot read, never dropped.
        Some(Body::Image(image)) => {
            decode_image(image).map_or(Decoded::Unsupported, Decoded::Image)
        }
        // Likewise a voice message that is not well formed.
        Some(Body::Audio(audio)) => {
            decode_audio(audio).map_or(Decoded::Unsupported, Decoded::Audio)
        }
        // Likewise a video message that is not well formed.
        Some(Body::Video(video)) => {
            decode_video(video).map_or(Decoded::Unsupported, Decoded::Video)
        }
        None => Decoded::Unsupported,
    };
    DecodedContent {
        sent_at: content.sent_at.as_ref().map(from_timestamp),
        body,
    }
}

/// The mentions of an encoded `Text` or `Image`, before the range checks.
fn mentions_from_pb(mentions: &[pb::Mention]) -> Vec<Mention> {
    mentions
        .iter()
        .filter_map(|mention| {
            let target = match mention.target.as_ref()? {
                pb::mention::Target::User(user) => MentionTarget::User(UserId::from_pb(user)),
                pb::mention::Target::Everyone(_) => MentionTarget::Everyone,
            };
            Some(Mention {
                target,
                start: usize::try_from(mention.start).ok()?,
                len: usize::try_from(mention.length).ok()?,
            })
        })
        .collect()
}

/// `mentions` as they go on the wire; fails if a range cannot be encoded.
fn mentions_to_pb(mentions: &[Mention]) -> Result<Vec<pb::Mention>, ContentError> {
    mentions
        .iter()
        .map(|mention| {
            Ok(pb::Mention {
                target: Some(match &mention.target {
                    MentionTarget::User(user) => pb::mention::Target::User(user.to_pb()),
                    MentionTarget::Everyone => {
                        pb::mention::Target::Everyone(pb::mention::Everyone {})
                    }
                }),
                start: u32::try_from(mention.start).map_err(|_| ContentError::TooLong)?,
                length: u32::try_from(mention.len).map_err(|_| ContentError::TooLong)?,
            })
        })
        .collect()
}

/// An `Image` body, if it is well formed: a 16-byte id, a 32-byte key, a
/// picture size within the limits, a ThumbHash and blob size within theirs
/// (message-content: Image messages). The caption's invalid mentions are
/// dropped (Image caption and mentions); the caption stays.
fn decode_image(image: pb::Image) -> Option<ImageContent> {
    use protocol::blob_store::{MAX_BLOB_BYTES, MAX_IMAGE_EDGE, MAX_THUMBHASH_BYTES};
    let edge = 1..=MAX_IMAGE_EDGE;
    let blob_size = image.blob_size;
    if !edge.contains(&image.width)
        || !edge.contains(&image.height)
        || image.thumbhash.len() > MAX_THUMBHASH_BYTES
        || blob_size == 0
        || blob_size > MAX_BLOB_BYTES as u64
    {
        return None;
    }
    let mentions = match &image.caption {
        Some(caption) => valid_mentions(caption, mentions_from_pb(&image.caption_mentions)),
        None => Vec::new(),
    };
    Some(ImageContent {
        blob_id: BlobId::try_from(image.blob_id.as_slice()).ok()?,
        blob_key: BlobKey::try_from(image.blob_key.as_slice()).ok()?,
        width: image.width,
        height: image.height,
        thumbhash: image.thumbhash,
        blob_size,
        caption: image.caption,
        mentions,
    })
}

/// An `Audio` body, if it is well formed: a 16-byte id, a 32-byte key, a blob size
/// within the cap, a duration from 1 ms to the longest clip and a waveform within
/// its limit (message-content: Audio messages).
fn decode_audio(audio: pb::Audio) -> Option<AudioContent> {
    use protocol::audio::{MAX_AUDIO_DURATION_MS, MAX_AUDIO_WAVEFORM_BYTES};
    use protocol::blob_store::MAX_BLOB_BYTES;
    if !(1..=MAX_AUDIO_DURATION_MS).contains(&audio.duration_ms)
        || audio.waveform.len() > MAX_AUDIO_WAVEFORM_BYTES
        || audio.blob_size == 0
        || audio.blob_size > MAX_BLOB_BYTES as u64
    {
        return None;
    }
    Some(AudioContent {
        blob_id: BlobId::try_from(audio.blob_id.as_slice()).ok()?,
        blob_key: BlobKey::try_from(audio.blob_key.as_slice()).ok()?,
        blob_size: audio.blob_size,
        duration_ms: audio.duration_ms,
        waveform: audio.waveform,
        reply: audio.reply_to.and_then(reply_from_pb),
    })
}

/// A `Video` body, if it is well formed: a 16-byte id, a 32-byte key, a blob size
/// within the ceiling, a duration from 1 ms to the 30-second cap, a non-zero
/// pixel size and a ThumbHash within its limit (message-content: Video messages).
/// The ceiling here is [`protocol::video::MAX_VIDEO_BYTES`]; the live per-server
/// cap is advertised and enforced at send time.
fn decode_video(video: pb::Video) -> Option<VideoContent> {
    use protocol::blob_store::MAX_THUMBHASH_BYTES;
    use protocol::video::{MAX_VIDEO_BYTES, MAX_VIDEO_DURATION_MS};
    if !(1..=MAX_VIDEO_DURATION_MS).contains(&video.duration_ms)
        || video.width == 0
        || video.height == 0
        || video.thumbhash.len() > MAX_THUMBHASH_BYTES
        || video.blob_size == 0
        || video.blob_size > MAX_VIDEO_BYTES as u64
    {
        return None;
    }
    Some(VideoContent {
        blob_id: BlobId::try_from(video.blob_id.as_slice()).ok()?,
        blob_key: BlobKey::try_from(video.blob_key.as_slice()).ok()?,
        blob_size: video.blob_size,
        duration_ms: video.duration_ms,
        width: video.width,
        height: video.height,
        thumbhash: video.thumbhash,
    })
}

/// A decoded `ReplyTo` from the wire, or `None` if its reference is malformed
/// (message-content: Replies; Reply quote describes the original's kind).
fn reply_from_pb(reply: pb::ReplyTo) -> Option<ReplyTo> {
    let sender = reply.sender?;
    Some(ReplyTo {
        user: UserId::from_pb(sender.user_id.as_ref()?),
        device: DeviceId::try_from(sender.device_id.as_slice()).ok()?,
        msg_id: <[u8; 16]>::try_from(reply.msg_id.as_slice()).ok()?,
        quote: protocol::message_content::reply_quote(&reply.quote),
        original_kind: kind_from_pb(reply.original_kind),
        original_duration_ms: reply.original_duration_ms,
        original_thumbhash: reply.original_thumbhash,
    })
}

/// The wire form of a `ReplyTo` (message-content: Replies; Reply quote describes
/// the original's kind). `quote` is cut to the limit here.
fn reply_to_pb(reply: &ReplyTo) -> pb::ReplyTo {
    pb::ReplyTo {
        sender: Some(pb::DeviceRef {
            user_id: Some(reply.user.to_pb()),
            device_id: reply.device.to_vec(),
        }),
        msg_id: reply.msg_id.to_vec(),
        quote: protocol::message_content::reply_quote(&reply.quote),
        original_kind: pb_original_kind(reply.original_kind) as i32,
        original_duration_ms: reply.original_duration_ms,
        original_thumbhash: reply.original_thumbhash.clone(),
    }
}

/// The wire enum for an original's kind.
fn pb_original_kind(kind: ReplyOriginalKind) -> pb::OriginalKind {
    match kind {
        ReplyOriginalKind::Text => pb::OriginalKind::Text,
        ReplyOriginalKind::Image => pb::OriginalKind::Image,
        ReplyOriginalKind::Audio => pb::OriginalKind::Audio,
    }
}

/// The original's kind from the wire; `UNSPECIFIED` and any unknown value read
/// as text, so an older sender's text reply renders as before.
fn kind_from_pb(value: i32) -> ReplyOriginalKind {
    match pb::OriginalKind::try_from(value) {
        Ok(pb::OriginalKind::Image) => ReplyOriginalKind::Image,
        Ok(pb::OriginalKind::Audio) => ReplyOriginalKind::Audio,
        _ => ReplyOriginalKind::Text,
    }
}

fn decode_text(text: pb::Text) -> TextContent {
    let mentions = mentions_from_pb(&text.mentions);
    let reply = text.reply_to.and_then(reply_from_pb);
    let mentions = valid_mentions(&text.text, mentions);
    TextContent {
        text: text.text,
        mentions,
        reply,
    }
}

/// Whether `mention`'s range is not empty, lies inside `text` and on
/// character boundaries (message-content: Mentions).
fn in_text(text: &str, mention: &Mention) -> bool {
    let Some(end) = mention.start.checked_add(mention.len) else {
        return false;
    };
    mention.len > 0 && text.get(mention.start..end).is_some()
}

/// The mentions a receiver keeps (message-content: Mentions): sorted by
/// `start`, each inside the text and on character boundaries, none
/// overlapping one kept before it. The rest are dropped; the text stays.
pub fn valid_mentions(text: &str, mut mentions: Vec<Mention>) -> Vec<Mention> {
    mentions.sort_by_key(|m| (m.start, m.len));
    let mut kept: Vec<Mention> = Vec::with_capacity(mentions.len());
    for mention in mentions {
        let after_previous = kept
            .last()
            .is_none_or(|previous| previous.start + previous.len <= mention.start);
        if in_text(text, &mention) && after_previous {
            kept.push(mention);
        } else {
            tracing::debug!("dropped a mention with a bad range");
        }
    }
    kept
}

/// Why a body cannot be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ContentError {
    /// It would encrypt to more than the ciphertext cap
    /// (message-content: Text only).
    #[error("the message is too long")]
    TooLong,
    /// A mention's range is empty, outside the text, not on character
    /// boundaries, or overlaps another (message-content: Mentions).
    #[error("a mention does not fit the text")]
    InvalidMention,
}

fn encode(body: Body, sent_at: SystemTime) -> Result<Vec<u8>, ContentError> {
    let encoded = pb::Content {
        sent_at: Some(timestamp(sent_at)),
        body: Some(body),
    }
    .encode_to_vec();
    if encoded.len() > MAX_CONTENT_BYTES {
        return Err(ContentError::TooLong);
    }
    Ok(encoded)
}

/// A `Text` body (message-content: Text only): UTF-8 text and nothing else.
pub fn encode_text(text: &str, sent_at: SystemTime) -> Result<Vec<u8>, ContentError> {
    encode_message(text, &[], None, sent_at)
}

/// A `Text` body with mentions and what it answers (message-content: Text
/// only; Mentions; Replies). Fails with [`ContentError::InvalidMention`] if
/// any mention is not one a receiver keeps. `reply.quote` is cut to the
/// limit here.
pub fn encode_message(
    text: &str,
    mentions: &[Mention],
    reply: Option<&ReplyTo>,
    sent_at: SystemTime,
) -> Result<Vec<u8>, ContentError> {
    if valid_mentions(text, mentions.to_vec()).len() != mentions.len() {
        return Err(ContentError::InvalidMention);
    }
    let mentions = mentions_to_pb(mentions)?;
    let reply_to = reply.map(reply_to_pb);
    encode(
        Body::Text(pb::Text {
            text: text.to_owned(),
            mentions,
            reply_to,
        }),
        sent_at,
    )
}

/// An `Image` body (message-content: Image messages; Image caption and
/// mentions). Fails with [`ContentError::InvalidMention`] if a caption mention
/// is not one a receiver keeps, and with [`ContentError::TooLong`] if it would
/// encrypt to more than the ciphertext cap.
pub fn encode_image(image: &ImageContent, sent_at: SystemTime) -> Result<Vec<u8>, ContentError> {
    let caption = image.caption.as_deref().unwrap_or_default();
    if valid_mentions(caption, image.mentions.clone()).len() != image.mentions.len() {
        return Err(ContentError::InvalidMention);
    }
    encode(
        Body::Image(pb::Image {
            blob_id: image.blob_id.to_vec(),
            blob_key: image.blob_key.to_vec(),
            width: image.width,
            height: image.height,
            thumbhash: image.thumbhash.clone(),
            blob_size: image.blob_size,
            caption: image.caption.clone(),
            caption_mentions: mentions_to_pb(&image.mentions)?,
        }),
        sent_at,
    )
}

/// An `Audio` body (message-content: Audio messages). Fails with
/// [`ContentError::TooLong`] if it would encrypt to more than the ciphertext cap.
pub fn encode_audio(audio: &AudioContent, sent_at: SystemTime) -> Result<Vec<u8>, ContentError> {
    encode(
        Body::Audio(pb::Audio {
            blob_id: audio.blob_id.to_vec(),
            blob_key: audio.blob_key.to_vec(),
            blob_size: audio.blob_size,
            duration_ms: audio.duration_ms,
            waveform: audio.waveform.clone(),
            reply_to: audio.reply.as_ref().map(reply_to_pb),
        }),
        sent_at,
    )
}

/// A `Video` body (message-content: Video messages). Fails with
/// [`ContentError::TooLong`] if it would encrypt to more than the ciphertext cap.
pub fn encode_video(video: &VideoContent, sent_at: SystemTime) -> Result<Vec<u8>, ContentError> {
    encode(
        Body::Video(pb::Video {
            blob_id: video.blob_id.to_vec(),
            blob_key: video.blob_key.to_vec(),
            blob_size: video.blob_size,
            duration_ms: video.duration_ms,
            width: video.width,
            height: video.height,
            thumbhash: video.thumbhash.clone(),
        }),
        sent_at,
    )
}

/// A `Profile` body for this device's own user (message-content: User-owned
/// profiles).
pub fn encode_profile(profile: &ProfileRow, sent_at: SystemTime) -> Result<Vec<u8>, ContentError> {
    encode(
        Body::Profile(pb::Profile {
            display_name: profile.display_name.clone(),
            about: profile.about.clone(),
        }),
        sent_at,
    )
}

/// A `GroupSettings` body (message-content: Group settings travel
/// encrypted); only admin devices send one.
pub fn encode_group_settings(
    name: Option<&str>,
    sent_at: SystemTime,
) -> Result<Vec<u8>, ContentError> {
    encode(
        Body::GroupSettings(pb::GroupSettings {
            name: name.map(str::to_owned),
        }),
        sent_at,
    )
}

/// `<username>@<server_id>` as a [`UserId`].
pub(crate) fn parse_user(text: &str) -> Option<UserId> {
    let (username, server_id) = text.split_once('@')?;
    Some(UserId::new(username, server_id))
}

/// The UI `Reply` for a decoded `ReplyTo`, with the original looked up in
/// `store` (client-runtime: Reply lookup) and the original's kind/duration
/// carried for the preview (message-content: Reply quote describes the
/// original's kind).
fn event_reply(store: &Store, reply: ReplyTo) -> Reply {
    Reply {
        original: store
            .chat_message_by_ref(&reply.device, &reply.msg_id)
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "cannot look up a reply's original");
                None
            }),
        to: reply.user,
        quote: reply.quote,
        kind: reply.original_kind,
        duration_ms: reply.original_duration_ms,
        thumbhash: reply.original_thumbhash,
    }
}

/// The body, mentions and reply of a chat message, with the reply's
/// original looked up in `store` (client-runtime: Reply lookup); `None` for
/// profile and group-settings bodies.
fn chat_parts(store: &Store, body: Decoded) -> Option<(MessageBody, Vec<Mention>, Option<Reply>)> {
    match body {
        Decoded::Text(content) => {
            let reply = content.reply.map(|reply| event_reply(store, reply));
            Some((MessageBody::Text(content.text), content.mentions, reply))
        }
        Decoded::Image(image) => {
            let body = image_body(store, &image);
            Some((MessageBody::Image(Box::new(body)), image.mentions, None))
        }
        Decoded::Audio(audio) => {
            let body = audio_body(store, &audio);
            // A voice message may itself answer another message (message-content:
            // Voice replies).
            let reply = audio.reply.map(|reply| event_reply(store, reply));
            Some((MessageBody::Audio(Box::new(body)), Vec::new(), reply))
        }
        Decoded::Video(video) => {
            let body = video_body(store, &video);
            Some((MessageBody::Video(Box::new(body)), Vec::new(), None))
        }
        Decoded::Unsupported => Some((MessageBody::Unsupported, Vec::new(), None)),
        Decoded::Profile(_) | Decoded::GroupSettings { .. } => None,
    }
}

/// An image message as the UI sees it: where its picture is on this device.
pub(crate) fn image_body(store: &Store, image: &ImageContent) -> crate::events::ImageBody {
    let state = if store.media_entry(&image.blob_id).ok().flatten().is_some() {
        MediaState::Available
    } else if store.is_media_expired(&image.blob_id).unwrap_or(false) {
        MediaState::Expired
    } else {
        MediaState::NotDownloaded
    };
    crate::events::ImageBody {
        blob_id: image.blob_id,
        width: image.width,
        height: image.height,
        thumbhash: image.thumbhash.clone(),
        size: image.blob_size,
        caption: image.caption.clone(),
        state,
    }
}

/// A voice message as the UI sees it: where its clip is on this device.
pub(crate) fn audio_body(store: &Store, audio: &AudioContent) -> crate::events::AudioBody {
    let state = if store.media_entry(&audio.blob_id).ok().flatten().is_some() {
        MediaState::Available
    } else if store.is_media_expired(&audio.blob_id).unwrap_or(false) {
        MediaState::Expired
    } else {
        MediaState::NotDownloaded
    };
    crate::events::AudioBody {
        blob_id: audio.blob_id,
        duration_ms: audio.duration_ms,
        waveform: audio.waveform.clone(),
        size: audio.blob_size,
        state,
    }
}

/// A video message as the UI sees it: where its clip is on this device.
pub(crate) fn video_body(store: &Store, video: &VideoContent) -> crate::events::VideoBody {
    let state = if store.media_entry(&video.blob_id).ok().flatten().is_some() {
        MediaState::Available
    } else if store.is_media_expired(&video.blob_id).unwrap_or(false) {
        MediaState::Expired
    } else {
        MediaState::NotDownloaded
    };
    crate::events::VideoBody {
        blob_id: video.blob_id,
        width: video.width,
        height: video.height,
        duration_ms: video.duration_ms,
        thumbhash: video.thumbhash.clone(),
        size: video.blob_size,
        state,
    }
}

/// Whether a message from `sender` with `mentions` mentions `own_user`
/// (message-content: Mentions): never for the user's own messages.
pub(crate) fn mentions_user(sender: &UserId, mentions: &[Mention], own_user: &UserId) -> bool {
    sender != own_user && mentions.iter().any(|m| m.names(own_user))
}

/// Whether the encoded `content` from `sender` mentions `own_user`.
fn content_mentions(content: &DecodedContent, sender: &UserId, own_user: &UserId) -> bool {
    match &content.body {
        Decoded::Text(text) => mentions_user(sender, &text.mentions, own_user),
        Decoded::Image(image) => mentions_user(sender, &image.mentions, own_user),
        _ => false,
    }
}

/// A stored chat message as the UI sees it; `None` for profile and
/// group-settings rows, which are not shown in the history. `own_user` is
/// this device's user.
pub(crate) fn to_message(
    store: &Store,
    stored: &StoredMessage,
    own_user: &UserId,
) -> Option<Message> {
    let decoded = decode(&stored.content);
    let (body, mentions, reply) = chat_parts(store, decoded.body)?;
    let sender = parse_user(&stored.sender_user)?;
    let own = stored.outgoing || sender == *own_user;
    Some(Message {
        id: stored.id,
        chat: crate::events::ChatId(stored.chat),
        own_user: own,
        mentions_me: !own && mentions_user(&sender, &mentions, own_user),
        sender,
        sender_device: stored.sender_device,
        outgoing: stored.outgoing,
        body,
        mentions,
        reply: reply.map(Box::new),
        sent_at: decoded.sent_at,
        accepted_at: stored.accepted_at,
        status: stored.status.clone(),
    })
}

/// What receiving one `APPLICATION` envelope did, for the event the engine
/// emits after the transaction commits (client-runtime: Events in order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Received {
    /// A chat message was stored.
    Message(Message),
    /// A user's stored profile changed (an equal one is [`Received::Nothing`]).
    Profile {
        /// The sending user.
        user: UserId,
        /// What they set.
        profile: ProfileRow,
    },
    /// The admin renamed a named group; the chat list shows the new name.
    ChatRenamed,
    /// Stored but nothing for the UI: a redelivery, `GroupSettings` from a
    /// non-admin device (which clients ignore), or a profile or group name
    /// equal to the stored one (a re-send after an add, qa-fixes D2).
    Nothing,
    /// It could not be read (it does not decrypt, or its MLS sender is not
    /// the device the server says sent it). Not dropped silently: the UI shows
    /// that a message could not be read.
    Unreadable {
        /// Who the server says sent it.
        sender: Option<UserId>,
        /// When the server accepted it.
        accepted_at: Option<SystemTime>,
        /// The chat it was for.
        chat: crate::events::ChatId,
    },
}

/// Decrypts an `APPLICATION` envelope, checks that its MLS sender is the
/// device the server names, stores it and applies a profile or group settings,
/// inside the caller's store transaction (client-runtime: Local message
/// store). `admin` is the admin's user id from the group context.
pub(crate) fn receive_application(
    store: &Store,
    group: &mut MlsGroup,
    envelope: &pb::Envelope,
    admin: Option<&str>,
    own_user: &UserId,
    now: SystemTime,
) -> Result<Received, EngineError> {
    // change `dms`: the chat the envelope's group is (0 for the main group).
    let chat = store.chat_of(&envelope.group_id)?;
    let seq = envelope.server_seq;
    let server_sender = envelope.sender.as_ref().and_then(|sender| {
        let user = UserId::from_pb(sender.user_id.as_ref()?);
        let device = DeviceId::try_from(sender.device_id.as_slice()).ok()?;
        Some((user, device))
    });
    let accepted_at = envelope.accepted_at.as_ref().map(from_timestamp);
    let unreadable = |why: &str| -> Result<Received, EngineError> {
        tracing::warn!(server_seq = seq, why, "cannot read an application message");
        Ok(Received::Unreadable {
            sender: server_sender.as_ref().map(|(user, _)| user.clone()),
            accepted_at,
            chat: crate::events::ChatId(chat),
        })
    };
    let Some((sender_user, sender_device)) = server_sender.clone() else {
        return unreadable("no sender");
    };
    let Ok(msg_id) = <[u8; 16]>::try_from(envelope.msg_id.as_slice()) else {
        return unreadable("a malformed msg_id");
    };
    let Some(message) = MlsMessageIn::tls_deserialize_exact(envelope.ciphertext.as_slice())
        .ok()
        .and_then(|m| m.try_into_protocol_message().ok())
    else {
        return unreadable("it does not parse");
    };
    let processed = match group.process_message(store.mls(), message) {
        Ok(processed) => processed,
        Err(error) => {
            tracing::debug!(server_seq = seq, %error, "decryption failed");
            return unreadable("it does not decrypt");
        }
    };
    // The MLS sender's certificate must name the device the server says sent it.
    let Sender::Member(leaf) = *processed.sender() else {
        return unreadable("not from a member");
    };
    let from_leaf = verify::member_claim(group, leaf).ok();
    if from_leaf.as_ref().is_none_or(|claim| {
        claim.claimed.user != sender_user || claim.claimed.device_id != sender_device
    }) {
        return unreadable("its MLS sender is not the device the server names");
    }
    let ProcessedMessageContent::ApplicationMessage(content) = processed.into_content() else {
        return unreadable("an APPLICATION envelope without an application message");
    };
    let content = content.into_bytes();
    let decoded = decode(&content);
    let sender_text = sender_user.to_string();
    let outcome = store.insert_incoming_in(
        chat,
        &IncomingMessage {
            server_seq: seq,
            msg_id,
            sender_user: &sender_text,
            sender_device,
            content: &content,
            accepted_at: accepted_at.unwrap_or(now),
            received_at: now,
            mentions_me: content_mentions(&decoded, &sender_user, own_user),
        },
    )?;
    let id = match outcome {
        InsertOutcome::Inserted(id) => id,
        InsertOutcome::Duplicate(_) => return Ok(Received::Nothing),
    };
    let sent_at = decoded.sent_at;
    Ok(match decoded.body {
        body @ (Decoded::Text(_)
        | Decoded::Image(_)
        | Decoded::Audio(_)
        | Decoded::Video(_)
        | Decoded::Unsupported) => {
            let Some((body, mentions, reply)) = chat_parts(store, body) else {
                return Ok(Received::Nothing);
            };
            Received::Message(Message {
                id,
                chat: crate::events::ChatId(chat),
                own_user: sender_user == *own_user,
                mentions_me: mentions_user(&sender_user, &mentions, own_user),
                sender: sender_user,
                sender_device,
                outgoing: false,
                body,
                mentions,
                reply: reply.map(Box::new),
                sent_at,
                accepted_at,
                status: None::<SendStatus>,
            })
        }
        Decoded::Profile(profile) => {
            // message-content: User-owned profiles. Applied to the sending
            // user only.
            let stored = store.profile(&sender_text)?;
            store.set_profile(&sender_text, &profile, now)?;
            if stored.unwrap_or_default() == profile {
                Received::Nothing
            } else {
                Received::Profile {
                    user: sender_user,
                    profile,
                }
            }
        }
        Decoded::GroupSettings { name } => {
            // message-content: Group settings travel encrypted. Only admin
            // devices change them.
            if admin != Some(sender_text.as_str()) {
                tracing::info!(
                    server_seq = seq,
                    "ignored group settings from a non-admin device"
                );
                Received::Nothing
            } else if chat == crate::store::MAIN_CHAT {
                // multi-group: The system lobby. Its name is fixed ("lobby");
                // a `GroupSettings` name for it is ignored.
                Received::Nothing
            } else {
                // A named group's name, from its admin.
                match name.as_deref() {
                    Some(name) if store.set_named_name(&envelope.group_id, name)? => {
                        Received::ChatRenamed
                    }
                    _ => Received::Nothing,
                }
            }
        }
    })
}
