//! Content bodies, and receiving application messages (client-core task
//! group 6; message-content; client-runtime: Events in order).
//!
//! The plaintext of every application message is an encoded
//! `vyajena.v1.Content` (message-content: Inner content format): a send time
//! and one of `Text`, `GroupSettings` or `Profile`. Only this core encodes or
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
    DeviceId, Mention, MentionTarget, Message, MessageBody, Reply, SendStatus, UserId,
};
use crate::identity::certificate::{from_timestamp, timestamp};
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

/// A `ReplyTo`, decoded (message-content: Replies).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyTo {
    /// The user who sent the original.
    pub user: UserId,
    /// The device that sent the original.
    pub device: DeviceId,
    /// The original envelope's `msg_id`.
    pub msg_id: [u8; 16],
    /// The quote, cut to `MAX_REPLY_QUOTE_CHARS`.
    pub quote: String,
}

/// What a `Content` says, decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decoded {
    /// A text message.
    Text(TextContent),
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
        None => Decoded::Unsupported,
    };
    DecodedContent {
        sent_at: content.sent_at.as_ref().map(from_timestamp),
        body,
    }
}

fn decode_text(text: pb::Text) -> TextContent {
    let mentions = text
        .mentions
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
        .collect();
    let reply = text.reply_to.and_then(|reply| {
        let sender = reply.sender?;
        Some(ReplyTo {
            user: UserId::from_pb(sender.user_id.as_ref()?),
            device: DeviceId::try_from(sender.device_id.as_slice()).ok()?,
            msg_id: <[u8; 16]>::try_from(reply.msg_id.as_slice()).ok()?,
            quote: protocol::message_content::reply_quote(&reply.quote),
        })
    });
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
    let mentions = mentions
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
        .collect::<Result<Vec<_>, ContentError>>()?;
    let reply_to = reply.map(|reply| pb::ReplyTo {
        sender: Some(pb::DeviceRef {
            user_id: Some(reply.user.to_pb()),
            device_id: reply.device.to_vec(),
        }),
        msg_id: reply.msg_id.to_vec(),
        quote: protocol::message_content::reply_quote(&reply.quote),
    });
    encode(
        Body::Text(pb::Text {
            text: text.to_owned(),
            mentions,
            reply_to,
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

/// The body, mentions and reply of a chat message, with the reply's
/// original looked up in `store` (client-runtime: Reply lookup); `None` for
/// profile and group-settings bodies.
fn chat_parts(store: &Store, body: Decoded) -> Option<(MessageBody, Vec<Mention>, Option<Reply>)> {
    match body {
        Decoded::Text(content) => {
            let reply = content.reply.map(|reply| Reply {
                original: store
                    .chat_message_by_ref(&reply.device, &reply.msg_id)
                    .unwrap_or_else(|error| {
                        tracing::warn!(%error, "cannot look up a reply's original");
                        None
                    }),
                to: reply.user,
                quote: reply.quote,
            });
            Some((MessageBody::Text(content.text), content.mentions, reply))
        }
        Decoded::Unsupported => Some((MessageBody::Unsupported, Vec::new(), None)),
        Decoded::Profile(_) | Decoded::GroupSettings { .. } => None,
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
        own_user: own,
        mentions_me: !own && mentions_user(&sender, &mentions, own_user),
        sender,
        sender_device: stored.sender_device,
        outgoing: stored.outgoing,
        body,
        mentions,
        reply,
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
    /// An admin device changed the group settings.
    GroupSettings {
        /// The group's name.
        name: Option<String>,
        /// The name stored before.
        previous: Option<String>,
        /// The sending (admin) user.
        by: UserId,
    },
    /// Stored but nothing for the UI: a redelivery, `GroupSettings` from a
    /// non-admin device (which clients ignore), or a profile or group name
    /// equal to the stored one (a re-send after an add, qa-fixes D2).
    Nothing,
    /// It could not be read (it does not decrypt, or its MLS sender is not
    /// the device the Pi says sent it). Not dropped silently: the UI shows
    /// that a message could not be read.
    Unreadable {
        /// Who the Pi says sent it.
        sender: Option<UserId>,
        /// When the Pi accepted it.
        accepted_at: Option<SystemTime>,
    },
}

/// Decrypts an `APPLICATION` envelope, checks that its MLS sender is the
/// device the Pi names, stores it and applies a profile or group settings,
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
    let seq = envelope.server_seq;
    let pi_sender = envelope.sender.as_ref().and_then(|sender| {
        let user = UserId::from_pb(sender.user_id.as_ref()?);
        let device = DeviceId::try_from(sender.device_id.as_slice()).ok()?;
        Some((user, device))
    });
    let accepted_at = envelope.accepted_at.as_ref().map(from_timestamp);
    let unreadable = |why: &str| -> Result<Received, EngineError> {
        tracing::warn!(server_seq = seq, why, "cannot read an application message");
        Ok(Received::Unreadable {
            sender: pi_sender.as_ref().map(|(user, _)| user.clone()),
            accepted_at,
        })
    };
    let Some((sender_user, sender_device)) = pi_sender.clone() else {
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
    // The MLS sender's certificate must name the device the Pi says sent it.
    let Sender::Member(leaf) = *processed.sender() else {
        return unreadable("not from a member");
    };
    let from_leaf = verify::member_claim(group, leaf).ok();
    if from_leaf.as_ref().is_none_or(|claim| {
        claim.claimed.user != sender_user || claim.claimed.device_id != sender_device
    }) {
        return unreadable("its MLS sender is not the device the Pi names");
    }
    let ProcessedMessageContent::ApplicationMessage(content) = processed.into_content() else {
        return unreadable("an APPLICATION envelope without an application message");
    };
    let content = content.into_bytes();
    let decoded = decode(&content);
    let sender_text = sender_user.to_string();
    let outcome = store.insert_incoming(&IncomingMessage {
        server_seq: seq,
        msg_id,
        sender_user: &sender_text,
        sender_device,
        content: &content,
        accepted_at: accepted_at.unwrap_or(now),
        received_at: now,
        mentions_me: content_mentions(&decoded, &sender_user, own_user),
    })?;
    let id = match outcome {
        InsertOutcome::Inserted(id) => id,
        InsertOutcome::Duplicate(_) => return Ok(Received::Nothing),
    };
    let sent_at = decoded.sent_at;
    Ok(match decoded.body {
        body @ (Decoded::Text(_) | Decoded::Unsupported) => {
            let Some((body, mentions, reply)) = chat_parts(store, body) else {
                return Ok(Received::Nothing);
            };
            Received::Message(Message {
                id,
                own_user: sender_user == *own_user,
                mentions_me: mentions_user(&sender_user, &mentions, own_user),
                sender: sender_user,
                sender_device,
                outgoing: false,
                body,
                mentions,
                reply,
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
            } else {
                let previous = store.group_name()?;
                store.set_group_name(name.as_deref(), now)?;
                if previous == name {
                    Received::Nothing
                } else {
                    Received::GroupSettings {
                        name,
                        previous,
                        by: sender_user,
                    }
                }
            }
        }
    })
}
