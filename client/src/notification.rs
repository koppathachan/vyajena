//! The summary a device builds, after it wakes and catches up, to show a local
//! notification (change `smarter-notifications`; local-notifications: The device
//! builds the notification locally).
//!
//! **Metadata only (v1).** A summary carries, per conversation with unread
//! messages, its *type* and an *unread count*, plus the admin-events that changed
//! the user's own access. It has no field for a sender, a message body, a caption
//! or any decrypted content, so the app cannot show one: the type system is the
//! guard. The type of a conversation comes from the wire `group_id` mapped to the
//! device's own conversation record; the count is the core's per-conversation
//! unread (what the chat list shows), recomputed on every call, never accumulated
//! from pushes.

use crate::events::{ChatId, ChatKind};

/// What kind of conversation a notification is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationKind {
    /// A DM.
    Dm,
    /// An admin-created named group.
    NamedGroup,
    /// The system lobby.
    Lobby,
}

impl ConversationKind {
    /// The type of a chat, from its local record alone (no decryption).
    pub(crate) fn of(kind: &ChatKind) -> Self {
        match kind {
            ChatKind::Lobby => Self::Lobby,
            ChatKind::Group { .. } => Self::NamedGroup,
            ChatKind::Dm { .. } => Self::Dm,
        }
    }
}

/// One conversation with unread messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationSummary {
    /// The conversation; the app uses it for grouping and to open it on a tap.
    pub chat: ChatId,
    /// Its type.
    pub kind: ConversationKind,
    /// A named group's local name (it travels encrypted, so only a local
    /// notification may show it); `None` for a DM, the lobby, or a group not yet
    /// named. Never a DM peer's name: that would be a sender.
    pub name: Option<String>,
    /// Unread messages, as the chat list counts them.
    pub unread: u32,
}

/// An admin-event that changed the user's own access
/// (local-notifications: Admin-event notifications are fixed, content-free
/// phrases). Routine membership churn is not one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminEventKind {
    /// The user was removed from a named group.
    Removed,
    /// A conversation the user is in was closed (a named group, or a DM whose
    /// peer was removed).
    Closed,
    /// The user was added to a new named group.
    Added,
}

impl AdminEventKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Removed => "removed",
            Self::Closed => "closed",
            Self::Added => "added",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "removed" => Some(Self::Removed),
            "closed" => Some(Self::Closed),
            "added" => Some(Self::Added),
            _ => None,
        }
    }
}

/// One pending admin-event, with what the app needs for its fixed phrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminEvent {
    /// Its id; pass the largest to [`crate::Client::clear_admin_events`] once
    /// notified.
    pub id: i64,
    /// What happened.
    pub kind: AdminEventKind,
    /// The conversation's type.
    pub conversation: ConversationKind,
    /// The conversation, if this device still has it (to open on a tap).
    pub chat: Option<ChatId>,
    /// A named group's local name, if known.
    pub name: Option<String>,
}

/// Everything the app needs to show this device's notifications right now.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NotificationSummary {
    /// Conversations with unread messages that may notify (not muted, not
    /// closed).
    pub conversations: Vec<ConversationSummary>,
    /// Pending admin-events that may notify.
    pub admin_events: Vec<AdminEvent>,
    /// The unread total across `conversations` (the app-icon badge at More).
    pub total_unread: u32,
}

/// A pending admin-event as stored.
pub(crate) struct StoredEvent {
    pub id: i64,
    pub kind: AdminEventKind,
    pub group_id: Vec<u8>,
}

/// Builds the summary from the chat list and the pending admin-events.
///
/// `muted_all` silences everything (notification-prefs: Mute the whole app); a
/// conversation muted on its own (`Chat::muted`) and a closed one raise no
/// unread notification. An admin-event is about the user's own access, so a
/// conversation's own mute does not silence it, only whole-app mute does. This is
/// the local gate point `mute-notifications` plugs into (design D8).
pub(crate) fn build(
    chats: &[crate::events::Chat],
    muted_all: bool,
    events: Vec<StoredEvent>,
    chat_of_group: impl Fn(&[u8]) -> Option<ChatId>,
) -> NotificationSummary {
    if muted_all {
        return NotificationSummary::default();
    }
    let mut conversations = Vec::new();
    let mut total_unread: u32 = 0;
    for chat in chats {
        let closed = matches!(
            chat.kind,
            ChatKind::Group { closed: true, .. } | ChatKind::Dm { closed: true, .. }
        );
        if chat.unread == 0 || chat.muted || closed {
            continue;
        }
        total_unread = total_unread.saturating_add(chat.unread);
        conversations.push(ConversationSummary {
            chat: chat.id,
            kind: ConversationKind::of(&chat.kind),
            name: match &chat.kind {
                ChatKind::Group { name, .. } => name.clone(),
                _ => None,
            },
            unread: chat.unread,
        });
    }
    let admin_events = events
        .into_iter()
        .map(|event| {
            let chat = chat_of_group(&event.group_id);
            let record = chat.and_then(|id| chats.iter().find(|c| c.id == id));
            AdminEvent {
                id: event.id,
                kind: event.kind,
                conversation: record
                    .map(|c| ConversationKind::of(&c.kind))
                    .unwrap_or(ConversationKind::NamedGroup),
                chat,
                name: match record.map(|c| &c.kind) {
                    Some(ChatKind::Group { name, .. }) => name.clone(),
                    _ => None,
                },
            }
        })
        .collect();
    NotificationSummary {
        conversations,
        admin_events,
        total_unread,
    }
}
