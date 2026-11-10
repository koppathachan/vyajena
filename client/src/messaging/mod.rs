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
use crate::events::{DeviceId, Message, MessageBody, MessageId, SendStatus, UserId};
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

/// What a `Content` says, decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decoded {
    /// A text message.
    Text(String),
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
        Some(Body::Text(text)) => Decoded::Text(text.text),
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

/// Why a body cannot be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ContentError {
    /// It would encrypt to more than the ciphertext cap
    /// (message-content: Text only).
    #[error("the message is too long")]
    TooLong,
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
    encode(
        Body::Text(pb::Text {
            text: text.to_owned(),
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

/// A stored chat message as the UI sees it; `None` for profile and
/// group-settings rows, which are not shown in the history.
pub(crate) fn to_message(stored: &StoredMessage) -> Option<Message> {
    let decoded = decode(&stored.content);
    let body = match decoded.body {
        Decoded::Text(text) => MessageBody::Text(text),
        Decoded::Unsupported => MessageBody::Unsupported,
        Decoded::Profile(_) | Decoded::GroupSettings { .. } => return None,
    };
    Some(Message {
        id: stored.id,
        sender: parse_user(&stored.sender_user)?,
        sender_device: stored.sender_device,
        outgoing: stored.outgoing,
        body,
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
    /// A user's profile changed.
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
    },
    /// Stored but nothing for the UI: a redelivery, or `GroupSettings` from
    /// a non-admin device, which clients ignore.
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
    let sender_text = sender_user.to_string();
    let outcome = store.insert_incoming(&IncomingMessage {
        server_seq: seq,
        msg_id,
        sender_user: &sender_text,
        sender_device,
        content: &content,
        accepted_at: accepted_at.unwrap_or(now),
        received_at: now,
    })?;
    let id = match outcome {
        InsertOutcome::Inserted(id) => id,
        InsertOutcome::Duplicate(_) => return Ok(Received::Nothing),
    };
    let decoded = decode(&content);
    Ok(match decoded.body {
        Decoded::Text(text) => Received::Message(received(
            id,
            sender_user,
            sender_device,
            MessageBody::Text(text),
            decoded.sent_at,
            accepted_at,
        )),
        Decoded::Unsupported => Received::Message(received(
            id,
            sender_user,
            sender_device,
            MessageBody::Unsupported,
            decoded.sent_at,
            accepted_at,
        )),
        Decoded::Profile(profile) => {
            // message-content: User-owned profiles. Applied to the sending
            // user only.
            store.set_profile(&sender_text, &profile, now)?;
            Received::Profile {
                user: sender_user,
                profile,
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
                store.set_group_name(name.as_deref(), now)?;
                Received::GroupSettings { name }
            }
        }
    })
}

fn received(
    id: MessageId,
    sender: UserId,
    sender_device: DeviceId,
    body: MessageBody,
    sent_at: Option<SystemTime>,
    accepted_at: Option<SystemTime>,
) -> Message {
    Message {
        id,
        sender,
        sender_device,
        outgoing: false,
        body,
        sent_at,
        accepted_at,
        status: None::<SendStatus>,
    }
}
