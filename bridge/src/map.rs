//! Core types to bridge types and back. Mapping only: no decisions.

use std::time::{SystemTime, UNIX_EPOCH};

use client::net::NetError;
use client::{
    ClientError, ConnectionState, DeviceId, Event, Member, Mention, Message, MessageBody,
    ReaddOutcome, SendStatus, UserId,
};

use crate::api::error::{CoreError, CoreErrorKind};
use crate::api::types::{
    AudioView, ChatBody, ChatKindInfo, ChatMessage, ChatSummary, ConnectionStatus, CoreEvent,
    ImageView, InviteInfo, MediaStatus, MemberDevice, MentionSpan, MentionTarget, ReaddAnswer,
    ReplyKind, ReplyQuote, SendState, UserRef,
};

pub(crate) fn user(user: &UserId) -> UserRef {
    UserRef {
        username: user.username.clone(),
        server_id: user.server_id.clone(),
    }
}

pub(crate) fn user_id(user: &UserRef) -> UserId {
    UserId::new(user.username.clone(), user.server_id.clone())
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn device_id(text: &str) -> Result<DeviceId, CoreError> {
    let bad = || CoreError::new(CoreErrorKind::InvalidPayload, "not a device id");
    let text = text.as_bytes();
    let mut id = DeviceId::default();
    if text.len() != id.len() * 2 {
        return Err(bad());
    }
    for (byte, pair) in id.iter_mut().zip(text.chunks_exact(2)) {
        let pair = std::str::from_utf8(pair).map_err(|_| bad())?;
        *byte = u8::from_str_radix(pair, 16).map_err(|_| bad())?;
    }
    Ok(id)
}

/// Milliseconds since the Unix epoch; times before it count as 0.
pub(crate) fn millis(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn status(status: SendStatus) -> SendState {
    match status {
        SendStatus::Sending => SendState::Sending,
        SendStatus::Accepted => SendState::Accepted,
        SendStatus::Failed { reason } => SendState::Failed { reason },
    }
}

fn mention_target(target: &client::MentionTarget) -> MentionTarget {
    match target {
        client::MentionTarget::User(u) => MentionTarget::User { user: user(u) },
        client::MentionTarget::Everyone => MentionTarget::Everyone,
    }
}

/// The core's mentions of `text` (UTF-8 bytes) as UTF-16 spans for Dart.
fn spans(text: &str, mentions: &[Mention]) -> Vec<MentionSpan> {
    mentions
        .iter()
        .filter_map(|m| {
            let (start, length) = crate::spans::to_utf16(text, m.start, m.len)?;
            Some(MentionSpan {
                target: mention_target(&m.target),
                start,
                length,
            })
        })
        .collect()
}

/// Dart's UTF-16 spans of `text` as the core's mentions; a span that does not
/// fall on character boundaries fails with `InvalidMention`.
pub(crate) fn mentions(text: &str, spans: &[MentionSpan]) -> Result<Vec<Mention>, CoreError> {
    spans
        .iter()
        .map(|span| {
            let (start, len) =
                crate::spans::to_utf8(text, span.start, span.length).ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::InvalidMention,
                        "a mention does not fit the text",
                    )
                })?;
            Ok(Mention {
                target: match &span.target {
                    MentionTarget::User { user } => client::MentionTarget::User(user_id(user)),
                    MentionTarget::Everyone => client::MentionTarget::Everyone,
                },
                start,
                len,
            })
        })
        .collect()
}

pub(crate) fn message(message: Message) -> ChatMessage {
    let mentions = match &message.body {
        MessageBody::Text(text) => spans(text, &message.mentions),
        // A caption's mentions are ranges of the caption.
        MessageBody::Image(image) => image
            .caption
            .as_deref()
            .map_or_else(Vec::new, |caption| spans(caption, &message.mentions)),
        _ => Vec::new(),
    };
    ChatMessage {
        id: message.id.0,
        chat_id: message.chat.0,
        sender: user(&message.sender),
        sender_device: hex(&message.sender_device),
        outgoing: message.outgoing,
        own_user: message.own_user,
        mentions,
        mentions_me: message.mentions_me,
        reply: message.reply.map(|reply| ReplyQuote {
            to: user(&reply.to),
            quote: reply.quote,
            kind: match reply.kind {
                client::ReplyOriginalKind::Image => ReplyKind::Image,
                client::ReplyOriginalKind::Audio => ReplyKind::Audio,
                client::ReplyOriginalKind::Text => ReplyKind::Text,
            },
            duration_ms: reply.duration_ms,
            thumbhash: reply.thumbhash,
            original_id: reply.original.map(|id| id.0),
            original_deleted: reply.original_deleted,
        }),
        body: match message.body {
            MessageBody::Text(text) => ChatBody::Text { text },
            MessageBody::Image(image) => {
                let image = *image;
                ChatBody::Image {
                    image: Box::new(ImageView {
                        width: image.width,
                        height: image.height,
                        thumbhash: image.thumbhash,
                        size: image.size,
                        caption: image.caption,
                        state: match image.state {
                            client::MediaState::Available => MediaStatus::Available,
                            client::MediaState::Expired => MediaStatus::Expired,
                            _ => MediaStatus::NotDownloaded,
                        },
                    }),
                }
            }
            MessageBody::Audio(audio) => {
                let audio = *audio;
                ChatBody::Audio {
                    audio: Box::new(AudioView {
                        duration_ms: audio.duration_ms,
                        waveform: audio.waveform,
                        size: audio.size,
                        state: match audio.state {
                            client::MediaState::Available => MediaStatus::Available,
                            client::MediaState::Expired => MediaStatus::Expired,
                            _ => MediaStatus::NotDownloaded,
                        },
                    }),
                }
            }
            MessageBody::Deleted {
                by_admin,
                revealable,
            } => ChatBody::Deleted {
                by_admin,
                revealable,
            },
            _ => ChatBody::Unsupported,
        },
        sent_at_ms: message.sent_at.map(millis),
        accepted_at_ms: message.accepted_at.map(millis),
        status: message.status.map(status),
    }
}

pub(crate) fn chat(chat: client::Chat) -> ChatSummary {
    ChatSummary {
        chat_id: chat.id.0,
        kind: match chat.kind {
            client::ChatKind::Lobby => ChatKindInfo::Lobby,
            client::ChatKind::Group { name, closed } => ChatKindInfo::Group { name, closed },
            client::ChatKind::Dm {
                peer,
                waiting,
                closed,
            } => ChatKindInfo::Dm {
                peer: user(&peer),
                waiting,
                closed,
            },
        },
        unread: chat.unread,
        unseen_mentions: chat.unseen_mentions,
        last_message_at_ms: chat.last_message_at.map(millis),
    }
}

pub(crate) fn member(member: &Member) -> MemberDevice {
    MemberDevice {
        user: user(&member.user),
        device: hex(&member.device),
    }
}

pub(crate) fn invite(invite: client::InviteInfo) -> InviteInfo {
    InviteInfo {
        text: (!invite.payload.is_empty()).then(|| crate::api::text::invite_text(&invite.payload)),
        code: invite.code,
        user: user(&invite.user),
        expires_at_ms: millis(invite.expires_at),
    }
}

pub(crate) fn event(event: Event) -> CoreEvent {
    match event {
        Event::Message(m) => CoreEvent::Message {
            message: message(m),
        },
        Event::SendStatus { id, status: s } => CoreEvent::SendStatus {
            id: id.0,
            status: status(s),
        },
        Event::ProfileChanged {
            user: u,
            display_name,
            about,
        } => CoreEvent::ProfileChanged {
            user: user(&u),
            display_name,
            about,
        },
        Event::Unreadable {
            sender,
            accepted_at,
            chat,
        } => CoreEvent::Unreadable {
            sender: sender.as_ref().map(user),
            accepted_at_ms: accepted_at.map(millis),
            chat_id: chat.0,
        },
        Event::Gap {
            newest_expired_at,
            chat,
        } => CoreEvent::Gap {
            newest_expired_at_ms: millis(newest_expired_at),
            chat_id: chat.0,
        },
        Event::ChatsChanged => CoreEvent::ChatsChanged,
        Event::RemovedForInactivity { missed_since } => CoreEvent::RemovedForInactivity {
            missed_since_ms: millis(missed_since),
        },
        Event::Rejoined => CoreEvent::Rejoined,
        Event::IdentityKeyChanged { user: u } => CoreEvent::IdentityKeyChanged { user: user(&u) },
        Event::ServerIdentityError => CoreEvent::ServerIdentityError,
        Event::Removed => CoreEvent::Removed,
        Event::JoinedGroup => CoreEvent::JoinedGroup,
        Event::MembersChanged {
            added,
            removed,
            by,
            automatic,
        } => CoreEvent::MembersChanged {
            added: added.iter().map(member).collect(),
            removed: removed.iter().map(member).collect(),
            by: by.as_ref().map(user),
            automatic: automatic.iter().map(member).collect(),
        },
        Event::CommitRefused { reason } => CoreEvent::CommitRefused { reason },
        Event::ReaddRequested { outcome } => CoreEvent::ReaddRequested {
            outcome: match outcome {
                ReaddOutcome::Asked => ReaddAnswer::Asked,
                ReaddOutcome::NotAMember => ReaddAnswer::NotAMember,
                ReaddOutcome::WelcomeQueued => ReaddAnswer::WelcomeQueued,
                ReaddOutcome::AlreadyWaiting => ReaddAnswer::AlreadyWaiting,
                ReaddOutcome::LimitReached => ReaddAnswer::LimitReached,
            },
        },
        Event::Connection(state) => CoreEvent::Connection {
            status: match state {
                ConnectionState::Connecting => ConnectionStatus::Connecting,
                ConnectionState::Connected => ConnectionStatus::Connected,
                _ => ConnectionStatus::Disconnected,
            },
        },
        _ => CoreEvent::Other,
    }
}

pub(crate) fn error(error: ClientError) -> CoreError {
    use CoreErrorKind as K;
    let kind = match &error {
        ClientError::KeyStore(_) => K::KeyStore,
        ClientError::Store(_) => K::Storage,
        ClientError::Net(NetError::ServerIdentity) => K::ServerIdentity,
        ClientError::Net(NetError::Rpc(_)) => K::Refused,
        ClientError::Net(_) => K::Network,
        ClientError::Payload(_) => K::InvalidPayload,
        ClientError::NotRegistered => K::NotRegistered,
        ClientError::AlreadyRegistered => K::AlreadyRegistered,
        ClientError::Offline => K::Offline,
        ClientError::NotAdmin => K::NotAdmin,
        ClientError::NotIdentityDevice => K::NotIdentityDevice,
        ClientError::NoLinkRequest => K::NoLinkRequest,
        ClientError::ServerMismatch(_) => K::ServerMismatch,
        ClientError::NoGroup => K::NoGroup,
        ClientError::AlreadyInGroup => K::AlreadyInGroup,
        ClientError::Stalled(_) => K::Stalled,
        ClientError::IdentityKeyChanged(_) => K::IdentityKeyChanged,
        ClientError::Certificate(_) => K::Certificate,
        ClientError::UnknownUser(_) => K::UnknownUser,
        ClientError::NoKeyPackages(_) => K::NoKeyPackages,
        ClientError::TooLarge => K::TooLarge,
        ClientError::TooLong => K::TooLong,
        ClientError::InvalidMention => K::InvalidMention,
        ClientError::NoSuchMessage => K::NoSuchMessage,
        ClientError::Rejected(_) => K::Rejected,
        ClientError::NotYet => K::NotYet,
        ClientError::NoSuchChat => K::NoSuchChat,
        ClientError::ChatClosed => K::ChatClosed,
        ClientError::DmsUnavailable => K::DmsUnavailable,
        ClientError::GroupsUnavailable => K::GroupsUnavailable,
        ClientError::MediaExpired => K::MediaExpired,
        ClientError::MediaNotOnDevice => K::MediaNotOnDevice,
        ClientError::Media(client::media::MediaError::Image(image)) => match image {
            client::media::ImageError::TooLarge => K::ImageTooLarge,
            _ => K::ImageUnsupported,
        },
        ClientError::Media(client::media::MediaError::Audio(audio)) => match audio {
            client::media::AudioError::TooLong | client::media::AudioError::TooLarge => {
                K::AudioTooLong
            }
            _ => K::AudioUnsupported,
        },
        _ => K::Internal,
    };
    CoreError::new(kind, error.to_string())
}
