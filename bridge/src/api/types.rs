//! The plain types Dart holds (flutter-app design D2). Times are milliseconds
//! since the Unix epoch, device ids lowercase hex, message ids the core's
//! local ids.

/// A user id `<username>@<server_id>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserRef {
    /// The local part, chosen by the admin.
    pub username: String,
    /// The server's server id.
    pub server_id: String,
}

/// Where an image or voice message's media is on this device (client-runtime: Image
/// download and cache; Expired media).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaStatus {
    /// Not downloaded: the blur shows and a tap downloads it.
    NotDownloaded,
    /// Cached on this device: it opens at once.
    Available,
    /// The server no longer has it: "media expired".
    Expired,
}

/// An image message (message-content: Image messages). No key and no blob id:
/// they stay in the core, which is given the message's local id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageView {
    /// Pixel width of the picture, to lay the bubble out before it loads.
    pub width: u32,
    /// Pixel height of the picture.
    pub height: u32,
    /// The ThumbHash blur to draw until the picture is downloaded.
    pub thumbhash: Vec<u8>,
    /// The picture's size on the server in bytes, to show before downloading.
    pub size: u64,
    /// The caption, only to be shown; its mentions are the message's
    /// `mentions`, as UTF-16 ranges of this text.
    pub caption: Option<String>,
    /// Where the picture is.
    pub state: MediaStatus,
}

/// Which on-device engine produced a subject cutout (photo-sticker:
/// Segmentation seam and RGBA contract). The app passes the source its platform
/// plugin used; the core encodes every source identically, so this only names
/// the producer. Only the two MVP sources are exposed; `Server`/`OnnxLocal` are
/// reserved in the core for later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StickerSource {
    /// iOS Vision foreground/subject mask.
    IosVision,
    /// Android ML Kit Subject Segmentation.
    AndroidMlKit,
}

impl StickerSource {
    pub(crate) fn to_core(self) -> client::media::SegmentationSource {
        match self {
            Self::IosVision => client::media::SegmentationSource::IosVision,
            Self::AndroidMlKit => client::media::SegmentationSource::AndroidMlKit,
        }
    }
}

/// A voice message (message-content: Audio messages). No key and no blob id: they
/// stay in the core, which is given the message's local id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioView {
    /// The clip's length in milliseconds.
    pub duration_ms: u32,
    /// The amplitude peaks, one byte each (255 the loudest), drawn as bars
    /// before the clip is downloaded.
    pub waveform: Vec<u8>,
    /// The clip's size on the server in bytes.
    pub size: u64,
    /// Where the clip is.
    pub state: MediaStatus,
}

/// A video message (message-content: Video messages). No key and no blob id: they
/// stay in the core, which is given the message's local id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoView {
    /// Pixel width of the clip, for the bubble's aspect ratio.
    pub width: u32,
    /// Pixel height of the clip, for the bubble's aspect ratio.
    pub height: u32,
    /// The clip's length in milliseconds.
    pub duration_ms: u32,
    /// The first-frame ThumbHash blur to draw until the clip is downloaded.
    pub thumbhash: Vec<u8>,
    /// The clip's size on the server in bytes, to show before downloading.
    pub size: u64,
    /// Where the clip is.
    pub state: MediaStatus,
}

/// What a message says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatBody {
    /// A text message.
    Text {
        /// The text, only to be shown.
        text: String,
    },
    /// An image with an optional caption.
    Image {
        /// The image. Boxed: it is large next to a text, and a message is
        /// carried in every event.
        image: Box<ImageView>,
    },
    /// A voice message.
    Audio {
        /// The clip. Boxed like an image: a message is carried in every event.
        audio: Box<AudioView>,
    },
    /// A video message.
    Video {
        /// The clip. Boxed like an image: a message is carried in every event.
        video: Box<VideoView>,
    },
    /// A deleted message, shown as a tombstone "This message was deleted"
    /// (message-deletion: Tombstone, not erasure).
    Deleted {
        /// An admin deleted another user's message in the lobby or a named
        /// group: the tombstone reads "by the admin".
        by_admin: bool,
        /// A DM tombstone whose original this device still holds: the app offers
        /// Reveal (message-deletion: Receiver may reveal in a DM). Always
        /// `false` in the lobby and named groups.
        revealable: bool,
    },
    /// A body this version does not know: shown as a placeholder saying the
    /// app needs an update (app-ui: Chat screen).
    Unsupported,
}

/// Who a mention points at (message-content: Mentions).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MentionTarget {
    /// One user.
    User {
        /// The user.
        user: UserRef,
    },
    /// Every member.
    Everyone,
}

/// One mention in a text, as a range of UTF-16 code units (Dart string
/// indices; mentions design D1).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MentionSpan {
    /// Who it points at.
    pub target: MentionTarget,
    /// First code unit of the range.
    pub start: u32,
    /// Code units in the range.
    pub length: u32,
}

/// What the answered message was, so the reply preview renders correctly even
/// without the original (message-content: Reply quote describes the original's
/// kind).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyKind {
    /// A text message: show the quoted text.
    Text,
    /// A picture: show a picture preview (its caption is the quote, if any).
    Image,
    /// A voice message: show a mic glyph and the duration.
    Audio,
    /// A video message: show a video glyph and "Video".
    Video,
}

/// What a reply answers (message-content: Replies; Reply quote describes the
/// original's kind).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyQuote {
    /// The user who sent the original.
    pub to: UserRef,
    /// The quote of the original, at most 100 characters, only to be shown
    /// (empty for a voice original; `kind` then says what to render).
    pub quote: String,
    /// What the original was, for the preview.
    pub kind: ReplyKind,
    /// For a voice original, its length in milliseconds (0 otherwise).
    pub duration_ms: u32,
    /// For a picture original, its ThumbHash (empty otherwise): the app decodes
    /// it to a blurred thumbnail for the quote, the same blur the image bubble
    /// shows, so it works without holding the original.
    pub thumbhash: Vec<u8>,
    /// The original's local id when this device has it (tap to jump).
    pub original_id: Option<i64>,
    /// The original was deleted and this device holds the delete, so the quote
    /// shows "deleted message" instead of the snapshot (message-deletion:
    /// Deleting media and replies).
    pub original_deleted: bool,
}

/// Where an own message stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendState {
    /// Stored; the server has not accepted it yet (⏳).
    Sending,
    /// The server accepted it (✓).
    Accepted,
    /// The server rejected it.
    Failed {
        /// Why, for display.
        reason: String,
    },
    /// Held because this device is wedged behind the group's epoch and out of
    /// sync (group-membership: Orphaned device self-recovery). Its text is
    /// kept and sent once the device re-syncs: shown as waiting to re-sync, not
    /// a silent ⏳ and not failed.
    OutOfSync,
}

/// A chat message, received or sent by this device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    /// The core's local id.
    pub id: i64,
    /// The chat it is in: 0 is the main group, else a DM (change `dms`).
    pub chat_id: i64,
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
    /// The mentions in a text body or an image's caption, in text order;
    /// empty otherwise.
    pub mentions: Vec<MentionSpan>,
    /// Whether it mentions this device's user or everyone (from another
    /// user): highlighted (app-ui: Mentions of me).
    pub mentions_me: bool,
    /// What it answers, if it is a reply.
    pub reply: Option<ReplyQuote>,
    /// The sender's clock when sent.
    pub sent_at_ms: Option<i64>,
    /// When the server accepted it; unset while sending.
    pub accepted_at_ms: Option<i64>,
    /// For own messages, where it stands; unset for received ones.
    pub status: Option<SendState>,
}

/// What a chat is (app-ui: Chat list, changes `dms`, `lobby-multi-group`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatKindInfo {
    /// The system lobby, shown as the fixed name "lobby".
    Lobby,
    /// An admin-created named group.
    Group {
        /// The name the admin set; unset until then (the app shows
        /// "Unnamed group").
        name: Option<String>,
        /// The admin closed it: history only.
        closed: bool,
    },
    /// A DM with `peer`.
    Dm {
        /// The other user.
        peer: UserRef,
        /// None of the peer's devices is in it yet: own messages wait.
        waiting: bool,
        /// The peer was removed: history only.
        closed: bool,
    },
}

/// One chat in the chat list (app-ui: Chat list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSummary {
    /// Its id: 0 is the lobby.
    pub chat_id: i64,
    /// Lobby, named group or DM.
    pub kind: ChatKindInfo,
    /// Received messages after the read mark.
    pub unread: u32,
    /// Mentions of this user not seen yet.
    pub unseen_mentions: u32,
    /// When its newest message was stored.
    pub last_message_at_ms: Option<i64>,
    /// This conversation is individually muted (notification-prefs: Mute a
    /// conversation); whole-app mute is shown globally via [`Core::app_muted`].
    pub muted: bool,
}

/// The connection to the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// Connecting or reconnecting with backoff.
    Connecting,
    /// Authenticated, with the stream open.
    Connected,
    /// Not connected; the core retries while started.
    Disconnected,
}

/// The server's answer when this device asked to be added again
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

/// A user's manual presence flag as the app shows it (manual-presence, change
/// `manual-presence`). `Unknown` means this device has learned no flag for the
/// user, and the app draws no dot rather than guessing a colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceStatus {
    /// The user chose to appear online (green).
    Online,
    /// The user chose to appear offline (red).
    Offline,
    /// No flag known for this user; show no dot.
    Unknown,
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
    /// A message arrived that this device cannot read.
    Unreadable {
        /// Who the server says sent it.
        sender: Option<UserRef>,
        /// When the server accepted it.
        accepted_at_ms: Option<i64>,
        /// The chat it was for.
        chat_id: i64,
    },
    /// Messages up to this time expired before this device fetched them.
    Gap {
        /// `accepted_at` of the newest expired message.
        newest_expired_at_ms: i64,
        /// The chat they were for.
        chat_id: i64,
    },
    /// The chat list changed: a DM was opened, joined, got its peer's first
    /// device, or was closed (change `dms`).
    ChatsChanged,
    /// The user's mute set changed (notification-prefs: Mute preference sync): a
    /// snapshot was adopted from the server, or this device's own mute/unmute
    /// took effect. The app re-reads the chats' `muted` flags and `app_muted`.
    MutePrefsChanged,
    /// This device was removed for inactivity; messages since then were not
    /// kept for it.
    RemovedForInactivity {
        /// Start of the missed period.
        missed_since_ms: i64,
    },
    /// This device was re-added after a removal.
    Rejoined,
    /// A chat is out of sync: this device is wedged behind the group's epoch
    /// and cannot self-recover, so the app shows an "out of sync — re-sync"
    /// affordance (app-ui: Out-of-sync re-sync affordance). Cleared on re-add
    /// (`Rejoined` / `JoinedGroup`).
    OutOfSync {
        /// The chat that is out of sync (0 is the lobby).
        chat_id: i64,
    },
    /// A known user's identity key changed.
    IdentityKeyChanged {
        /// The user.
        user: UserRef,
    },
    /// The server's TLS key does not match the pin; the core stopped connecting.
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
    /// This device asked the server to be added again (group-membership: Re-add
    /// after an unusable Welcome).
    ReaddRequested {
        /// What the server answered.
        outcome: ReaddAnswer,
    },
    /// The connection state changed.
    Connection {
        /// The new state.
        status: ConnectionStatus,
    },
    /// A manual presence flag changed (manual-presence: change `manual-presence`):
    /// `user` is unset when this device's own flag changed, and set to the peer
    /// whose flag was updated otherwise. The app re-reads `my_presence` or
    /// `presence_of`.
    PresenceChanged {
        /// The peer whose flag changed, or unset for this user's own flag.
        user: Option<UserRef>,
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
