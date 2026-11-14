//! Core types to bridge types and back. Mapping only: no decisions.

use std::time::{SystemTime, UNIX_EPOCH};

use client::net::NetError;
use client::{
    ClientError, ConnectionState, DeviceId, Event, Member, Message, MessageBody, SendStatus, UserId,
};

use crate::api::error::{CoreError, CoreErrorKind};
use crate::api::types::{
    ChatBody, ChatMessage, ConnectionStatus, CoreEvent, InviteInfo, MemberDevice, SendState,
    UserRef,
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

pub(crate) fn message(message: Message) -> ChatMessage {
    ChatMessage {
        id: message.id.0,
        sender: user(&message.sender),
        sender_device: hex(&message.sender_device),
        outgoing: message.outgoing,
        own_user: message.own_user,
        body: match message.body {
            MessageBody::Text(text) => ChatBody::Text { text },
            _ => ChatBody::Unsupported,
        },
        sent_at_ms: message.sent_at.map(millis),
        accepted_at_ms: message.accepted_at.map(millis),
        status: message.status.map(status),
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
        Event::GroupSettingsChanged { name, previous, by } => CoreEvent::GroupSettingsChanged {
            name,
            previous,
            by: user(&by),
        },
        Event::Unreadable {
            sender,
            accepted_at,
        } => CoreEvent::Unreadable {
            sender: sender.as_ref().map(user),
            accepted_at_ms: accepted_at.map(millis),
        },
        Event::Gap { newest_expired_at } => CoreEvent::Gap {
            newest_expired_at_ms: millis(newest_expired_at),
        },
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
        ClientError::Rejected(_) => K::Rejected,
        ClientError::NotYet => K::NotYet,
        _ => K::Internal,
    };
    CoreError::new(kind, error.to_string())
}
