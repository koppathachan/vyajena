//! The plain types Dart holds (flutter-app design D2). Times are milliseconds
//! since the Unix epoch, device ids lowercase hex, message ids the core's
//! local ids.

/// A user id `<username>@<server_id>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserRef {
    /// The local part, chosen by the admin.
    pub username: String,
    /// The Pi's server id.
    pub server_id: String,
}

/// What a message says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatBody {
    /// A text message.
    Text {
        /// The text, only to be shown.
        text: String,
    },
    /// A body this version does not know: shown as a placeholder saying the
    /// app needs an update (app-ui: Chat screen).
    Unsupported,
}

/// Where an own message stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendState {
    /// Stored; the Pi has not accepted it yet (⏳).
    Sending,
    /// The Pi accepted it (✓).
    Accepted,
    /// The Pi rejected it.
    Failed {
        /// Why, for display.
        reason: String,
    },
}

/// A chat message, received or sent by this device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    /// The core's local id.
    pub id: i64,
    /// The sending user.
    pub sender: UserRef,
    /// The sending device, hex.
    pub sender_device: String,
    /// Whether this device sent it (only these show ⏳/✓).
    pub outgoing: bool,
    /// Whether this device's user sent it, from any of their devices: shown
    /// right-aligned with no sender label (app-ui: Chat screen).
    pub own_user: bool,
    /// The body.
    pub body: ChatBody,
    /// The sender's clock when sent.
    pub sent_at_ms: Option<i64>,
    /// When the Pi accepted it; unset while sending.
    pub accepted_at_ms: Option<i64>,
    /// For own messages, where it stands; unset for received ones.
    pub status: Option<SendState>,
}

/// The connection to the Pi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// Connecting or reconnecting with backoff.
    Connecting,
    /// Authenticated, with the stream open.
    Connected,
    /// Not connected; the core retries while started.
    Disconnected,
}

/// The Pi's answer when this device asked to be added again
/// (group-membership: Re-add after an unusable Welcome).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaddAnswer {
    /// The admin's device (or another of the user's) will re-add it.
    Asked,
    /// Not in the group yet: it waits for its first add.
    NotAMember,
    /// A Welcome for it is still on its way.
    WelcomeQueued,
    /// It already waits to be re-added.
    AlreadyWaiting,
    /// It asked too often today; it asks again at a later start.
    LimitReached,
}

/// Which push service a token is for (device-identity: Push token
/// registration).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushPlatform {
    /// Firebase Cloud Messaging (Android).
    Fcm,
    /// Apple Push Notification service (iOS).
    Apns,
}

/// One event from the core, in the core's order (client-runtime: Events in
/// order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreEvent {
    /// A message was received and stored.
    Message {
        /// The message.
        message: ChatMessage,
    },
    /// An own message changed status.
    SendStatus {
        /// The message's local id.
        id: i64,
        /// Its new status.
        status: SendState,
    },
    /// A user's stored profile changed: a new value, or dropped after
    /// `IdentityKeyChanged` (both unset). An equal re-send emits nothing.
    ProfileChanged {
        /// The user.
        user: UserRef,
        /// The display name, if set.
        display_name: Option<String>,
        /// The "about" text, if set.
        about: Option<String>,
    },
    /// The admin changed the group settings; emitted only when the name
    /// changes.
    GroupSettingsChanged {
        /// The group's name, if set.
        name: Option<String>,
        /// The name stored before; unset on this device's first sync, when
        /// no rename notice is shown (app-ui: Notices in the chat).
        previous: Option<String>,
        /// The user whose admin device renamed it.
        by: UserRef,
    },
    /// A message arrived that this device cannot read.
    Unreadable {
        /// Who the Pi says sent it.
        sender: Option<UserRef>,
        /// When the Pi accepted it.
        accepted_at_ms: Option<i64>,
    },
    /// Messages up to this time expired before this device fetched them.
    Gap {
        /// `accepted_at` of the newest expired message.
        newest_expired_at_ms: i64,
    },
    /// This device was removed for inactivity; messages since then were not
    /// kept for it.
    RemovedForInactivity {
        /// Start of the missed period.
        missed_since_ms: i64,
    },
    /// This device was re-added after a removal.
    Rejoined,
    /// A known user's identity key changed.
    IdentityKeyChanged {
        /// The user.
        user: UserRef,
    },
    /// The Pi's TLS key does not match the pin; the core stopped connecting.
    ServerIdentityError,
    /// This device was removed from the group (revoked); the core stopped
    /// connecting and history stays readable.
    Removed,
    /// This device joined the group from a Welcome.
    JoinedGroup,
    /// The group's member devices changed. Empty lists when this device
    /// created or joined the group (no notices for its own join).
    MembersChanged {
        /// Devices added.
        added: Vec<MemberDevice>,
        /// Devices removed.
        removed: Vec<MemberDevice>,
        /// The committer's user; unset when this device joined.
        by: Option<UserRef>,
        /// Those of `added` and `removed` that changed automatically
        /// (inactivity Removes and re-adds): no notice for them.
        automatic: Vec<MemberDevice>,
    },
    /// This device refused a commit and cannot follow the group until
    /// re-added.
    CommitRefused {
        /// Why, for display.
        reason: String,
    },
    /// This device asked the Pi to be added again (group-membership: Re-add
    /// after an unusable Welcome).
    ReaddRequested {
        /// What the Pi answered.
        outcome: ReaddAnswer,
    },
    /// The connection state changed.
    Connection {
        /// The new state.
        status: ConnectionStatus,
    },
    /// An event this bridge version does not know yet.
    Other,
}

/// A user's profile.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileInfo {
    /// The display name, if set.
    pub display_name: Option<String>,
    /// The "about" text, if set.
    pub about: Option<String>,
}

/// A group member device.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemberDevice {
    /// The device's user.
    pub user: UserRef,
    /// The device, hex.
    pub device: String,
}

/// An invite the admin created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteInfo {
    /// The single-use code (for revoking).
    pub code: String,
    /// The user id it creates.
    pub user: UserRef,
    /// When it expires unless used.
    pub expires_at_ms: i64,
    /// The invite text to share (design D5); unset in `list_invites`.
    pub text: Option<String>,
}

/// What an invite text names, shown before joining (app-ui: Join from an
/// invite).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvitePreview {
    /// The user id the invite creates.
    pub user: UserRef,
    /// The relay's `host:port`.
    pub relay_address: String,
}
