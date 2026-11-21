//! The plain types a UI sees: events, messages, ids (client-core design D2).
//!
//! No OpenMLS or protobuf type appears here, so `flutter_rust_bridge` can wrap
//! them unchanged. The enums are `#[non_exhaustive]`: later task groups add
//! variants as they implement the features behind them.

use std::fmt;
use std::time::SystemTime;

use tokio::sync::mpsc;

/// A device id: the first 16 bytes of SHA-256 over the device auth key
/// (device-identity: User identity key and device certificates).
pub type DeviceId = [u8; protocol::device_identity::DEVICE_ID_LEN];

/// A user id `<username>@<server_id>` (device-identity: Federation-ready
/// identifiers).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UserId {
    /// The local part, chosen by the admin.
    pub username: String,
    /// The server's server id.
    pub server_id: String,
}

impl UserId {
    /// `<username>@<server_id>`.
    pub fn new(username: impl Into<String>, server_id: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            server_id: server_id.into(),
        }
    }

    pub(crate) fn to_pb(&self) -> crate::pb::UserId {
        crate::pb::UserId {
            username: self.username.clone(),
            server_id: self.server_id.clone(),
        }
    }

    pub(crate) fn from_pb(user: &crate::pb::UserId) -> Self {
        Self::new(user.username.clone(), user.server_id.clone())
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.username, self.server_id)
    }
}

/// A message's id in the local store: stable for the life of the database and
/// the handle a UI uses for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MessageId(pub i64);

/// Where an own message stands (client-runtime: Send status).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendStatus {
    /// Stored locally; the server has not accepted it yet.
    Sending,
    /// The server accepted it, which means it is durable there.
    Accepted,
    /// The server rejected it for a reason other than a stale epoch.
    Failed {
        /// Why, for display.
        reason: String,
    },
}

/// Where an image or voice message's media is on this device (client-runtime: Image
/// download and cache; Expired media).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaState {
    /// Not downloaded: the blur shows, and a tap downloads it.
    NotDownloaded,
    /// In this device's media cache: it opens at once.
    Available,
    /// The server no longer has it ("media expired").
    Expired,
}

/// An image message, as the UI sees it (message-content: Image messages). The
/// blob key is not here: it never leaves the core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageBody {
    /// The blob's id, which names the image to [`crate::Client::download_image`]
    /// callers and keys the cache.
    pub blob_id: [u8; protocol::blob_store::BLOB_ID_LEN],
    /// Pixel width of the picture.
    pub width: u32,
    /// Pixel height of the picture.
    pub height: u32,
    /// The ThumbHash blur shown until the picture is downloaded.
    pub thumbhash: Vec<u8>,
    /// The blob's size on the server, in bytes.
    pub size: u64,
    /// The caption, if there is one. Its mentions are the message's
    /// [`Message::mentions`].
    pub caption: Option<String>,
    /// Where the picture is.
    pub state: MediaState,
}

/// A voice message, as the UI sees it (message-content: Audio messages). The blob
/// key is not here: it never leaves the core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioBody {
    /// The blob's id, which keys the cache.
    pub blob_id: [u8; protocol::blob_store::BLOB_ID_LEN],
    /// The clip's length in milliseconds.
    pub duration_ms: u32,
    /// The amplitude peaks, one byte each (255 the loudest), drawn as bars before
    /// the clip is downloaded.
    pub waveform: Vec<u8>,
    /// The blob's size on the server, in bytes.
    pub size: u64,
    /// Where the clip is.
    pub state: MediaState,
}

/// What a message says.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MessageBody {
    /// A text message (message-content: Text only).
    Text(String),
    /// An image with an optional caption (message-content: Image messages).
    /// Boxed: a picture's description is large next to a text, and `Message`
    /// is carried in every event.
    Image(Box<ImageBody>),
    /// A voice message (message-content: Audio messages). Boxed like an image.
    Audio(Box<AudioBody>),
    /// A body this version does not know: shown as "needs a newer app
    /// version" (message-content: Inner content format).
    Unsupported,
}

/// Who a mention points at (message-content: Mentions).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MentionTarget {
    /// One user.
    User(UserId),
    /// Every member; any member may send it.
    Everyone,
}

/// One mention in a text: its target and the UTF-8 byte range of the text
/// it covers (message-content: Mentions).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Mention {
    /// Who it points at.
    pub target: MentionTarget,
    /// UTF-8 byte offset of the range in the text.
    pub start: usize,
    /// UTF-8 byte length of the range.
    pub len: usize,
}

impl Mention {
    /// Whether it names `user` or everyone.
    pub fn names(&self, user: &UserId) -> bool {
        match &self.target {
            MentionTarget::User(target) => target == user,
            MentionTarget::Everyone => true,
        }
    }
}

/// What a reply answers (message-content: Replies; client-runtime: Reply
/// lookup).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// The user who sent the original.
    pub to: UserId,
    /// The quote of the original the replier's core sent.
    pub quote: String,
    /// The original's local id, when this device has it stored.
    pub original: Option<MessageId>,
}

/// A chat: the lobby, a named group or a DM. The lobby is
/// [`ChatId::MAIN`]; the other ids are local to this device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChatId(pub i64);

impl ChatId {
    /// The main group.
    pub const MAIN: ChatId = ChatId(0);

    /// Whether this is the main group.
    pub fn is_main(self) -> bool {
        self == Self::MAIN
    }
}

/// What a chat is (app-ui: Chat list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatKind {
    /// The system lobby: the one all-members group (multi-group: The system
    /// lobby). Shown as the fixed name "lobby".
    Lobby,
    /// An admin-created named group (multi-group).
    Group {
        /// The name the admin set; `None` until then (the UI shows a
        /// placeholder).
        name: Option<String>,
        /// The server closed it. History only.
        closed: bool,
    },
    /// A DM with `peer` (direct-messages).
    Dm {
        /// The other user.
        peer: UserId,
        /// None of the peer's devices is in it yet: messages wait to be sent
        /// ("Waiting for Bobby's devices").
        waiting: bool,
        /// The server closed it: the peer was removed. History only.
        closed: bool,
    },
}

/// One chat in the chat list (app-ui: Chat list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chat {
    /// Its id.
    pub id: ChatId,
    /// Main group or DM.
    pub kind: ChatKind,
    /// Received chat messages after the read mark.
    pub unread: u32,
    /// Mentions of this user not seen yet.
    pub unseen_mentions: u32,
    /// When its newest chat message was stored.
    pub last_message_at: Option<SystemTime>,
}

/// A chat message, received or sent by this device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Local id.
    pub id: MessageId,
    /// The chat it is in.
    pub chat: ChatId,
    /// The sending user.
    pub sender: UserId,
    /// The sending device.
    pub sender_device: DeviceId,
    /// Whether this device sent it.
    pub outgoing: bool,
    /// Whether this device's user sent it, from this or another of their
    /// devices (qa-fixes D1: shown as an own message, with no sender label).
    pub own_user: bool,
    /// The body.
    pub body: MessageBody,
    /// The valid mentions in a text body, in text order (message-content:
    /// Mentions); empty for other bodies.
    pub mentions: Vec<Mention>,
    /// Whether a mention names this device's user or everyone, and another
    /// user sent it.
    pub mentions_me: bool,
    /// What it answers, if it is a reply (message-content: Replies).
    pub reply: Option<Reply>,
    /// The sender's clock when it was sent.
    pub sent_at: Option<SystemTime>,
    /// When the server accepted it; `None` while sending.
    pub accepted_at: Option<SystemTime>,
    /// For own messages, where it stands; `None` for received ones.
    pub status: Option<SendStatus>,
}

/// The connection to the server, as the UI shows it (client-runtime: Connection
/// lifecycle).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConnectionState {
    /// Connecting or reconnecting with backoff.
    Connecting,
    /// Authenticated, with the `Connect` stream open.
    Connected,
    /// Not connected; the core will retry.
    Disconnected,
}

/// One event, delivered in order on the [`EventStream`] (client-runtime:
/// Events in order).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    /// A message was received and stored.
    Message(Message),
    /// An own message changed status.
    SendStatus {
        /// The message.
        id: MessageId,
        /// Its new status.
        status: SendStatus,
    },
    /// A user's stored profile changed (message-content: User-owned
    /// profiles): a `Profile` with a new value, or the profile dropped after
    /// [`Event::IdentityKeyChanged`] (both fields `None`). A `Profile` equal to
    /// the stored one (a re-send after an add) emits nothing.
    ProfileChanged {
        /// The user whose device sent it.
        user: UserId,
        /// The display name, if set.
        display_name: Option<String>,
        /// The "about" text, if set.
        about: Option<String>,
    },
    /// A message arrived that this device cannot read: it does not decrypt,
    /// or its MLS sender is not the device the server says sent it. Shown as a
    /// placeholder; nothing is dropped silently.
    Unreadable {
        /// Who the server says sent it.
        sender: Option<UserId>,
        /// When the server accepted it.
        accepted_at: Option<SystemTime>,
        /// The chat it was for.
        chat: ChatId,
    },
    /// Messages up to this time expired on the server before this device fetched
    /// them (message-delivery: Expiry and gap notice).
    Gap {
        /// `accepted_at` of the newest expired message.
        newest_expired_at: SystemTime,
        /// The chat they were for.
        chat: ChatId,
    },
    /// The chat list changed (change `dms`): a DM was opened or joined, the
    /// peer's first device was added, or the server closed one.
    ChatsChanged,
    /// This device was removed for inactivity; messages since `missed_since`
    /// were not kept for it (group-membership: Automatic re-add).
    RemovedForInactivity {
        /// Start of the missed period.
        missed_since: SystemTime,
    },
    /// This device was re-added to the group after a removal.
    Rejoined,
    /// A known user's identity key changed; adding their devices is blocked
    /// until the user accepts the new key (key-directory: Clients verify the
    /// identity chain).
    IdentityKeyChanged {
        /// The user.
        user: UserId,
    },
    /// The server presented a TLS key that does not match the pin (client-runtime:
    /// Connection lifecycle). The core stops connecting until
    /// [`crate::Client::start`] is called again.
    ServerIdentityError,
    /// The server answered `Authenticate` or the stream with `PERMISSION_DENIED`:
    /// this device was revoked (removed from the group by the admin, or by
    /// another device of its user). Emitted once; the core stops connecting
    /// and local history stays readable (client-runtime: Connection
    /// lifecycle).
    Removed,
    /// This device joined the group from a Welcome for the first time.
    JoinedGroup,
    /// The group's member devices changed (a commit was merged, or this
    /// device joined or created the group, with empty lists).
    MembersChanged {
        /// Devices the commit added.
        added: Vec<Member>,
        /// Devices the commit removed.
        removed: Vec<Member>,
        /// The committer's user; `None` when this device joined from a
        /// Welcome.
        by: Option<UserId>,
        /// The devices in `added` or `removed` that changed automatically:
        /// removed by the server's inactivity Remove, or re-added after one (the
        /// same device id). The UI shows no notice for them (qa-fixes D5).
        automatic: Vec<Member>,
    },
    /// This device refused a commit or Welcome the server delivered: a leaf's
    /// certificate, its key or the commit-authority rule did not check out
    /// (group-membership: Device-level membership; Commit authority). It
    /// cannot follow the group until it is re-added.
    CommitRefused {
        /// Why, for display.
        reason: String,
    },
    /// This device asked the server to be added again after it could not use a
    /// Welcome, or because it had no group at start (group-membership:
    /// Re-add after an unusable Welcome). Only `Asked` and `LimitReached`
    /// change what the device waits for.
    ReaddRequested {
        /// What the server answered.
        outcome: ReaddOutcome,
    },
    /// The connection state changed.
    Connection(ConnectionState),
}

/// The server's answer to `Keys.RequestReadd` (group-membership: Re-add after an
/// unusable Welcome).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaddOutcome {
    /// The server will have this device re-added; it waits for the new Welcome.
    Asked,
    /// It is not in the group yet: it waits for its first add as usual.
    NotAMember,
    /// A Welcome for it is still on its way.
    WelcomeQueued,
    /// It already waits to be re-added.
    AlreadyWaiting,
    /// It asked too often today; it asks again at a later start.
    LimitReached,
}

/// A user's profile (message-content: User-owned profiles).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profile {
    /// The display name, if set.
    pub display_name: Option<String>,
    /// The "about" text, if set.
    pub about: Option<String>,
}

/// A group member device, as the UI lists it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Member {
    /// The device's user.
    pub user: UserId,
    /// The device.
    pub device: DeviceId,
}

/// An invite the admin created (device-identity: Invites).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteInfo {
    /// The single-use code.
    pub code: String,
    /// The user id it creates.
    pub user: UserId,
    /// When it expires unless used.
    pub expires_at: SystemTime,
    /// The invite payload to show as a QR code or text: relay address, TLS
    /// pin, code and user id. Empty in [`crate::Client::list_invites`].
    pub payload: Vec<u8>,
}

/// Where a push token comes from (device-identity: Push token registration).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushPlatform {
    /// Firebase Cloud Messaging.
    Fcm,
    /// Apple Push Notification service.
    Apns,
}

/// A push token as the platform SDK hands it over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushToken {
    /// The platform.
    pub platform: PushPlatform,
    /// The token.
    pub token: String,
}

/// The one ordered event stream of a [`crate::Client`].
#[derive(Debug)]
pub struct EventStream {
    rx: mpsc::UnboundedReceiver<Event>,
}

impl EventStream {
    /// The next event, or `None` once the client is gone.
    pub async fn next(&mut self) -> Option<Event> {
        self.rx.recv().await
    }

    /// The next event if one is ready, without waiting.
    pub fn try_next(&mut self) -> Option<Event> {
        self.rx.try_recv().ok()
    }
}

/// The sending half, kept by the client.
#[derive(Debug, Clone)]
pub(crate) struct EventSender {
    tx: mpsc::UnboundedSender<Event>,
}

impl EventSender {
    /// Emits `event`. Emitting after the UI dropped the stream is not an
    /// error: there is nobody left to tell.
    pub(crate) fn emit(&self, event: Event) {
        let _ = self.tx.send(event);
    }
}

/// A connected sender and stream.
pub(crate) fn channel() -> (EventSender, EventStream) {
    let (tx, rx) = mpsc::unbounded_channel();
    (EventSender { tx }, EventStream { rx })
}
