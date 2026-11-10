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
    /// The Pi's server id.
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
    /// Stored locally; the Pi has not accepted it yet.
    Sending,
    /// The Pi accepted it, which means it is durable there.
    Accepted,
    /// The Pi rejected it for a reason other than a stale epoch.
    Failed {
        /// Why, for display.
        reason: String,
    },
}

/// What a message says.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MessageBody {
    /// A text message (message-content: Text only).
    Text(String),
    /// A body this version does not know: shown as "needs a newer app
    /// version" (message-content: Inner content format).
    Unsupported,
}

/// A chat message, received or sent by this device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Local id.
    pub id: MessageId,
    /// The sending user.
    pub sender: UserId,
    /// The sending device.
    pub sender_device: DeviceId,
    /// Whether this device sent it.
    pub outgoing: bool,
    /// The body.
    pub body: MessageBody,
    /// The sender's clock when it was sent.
    pub sent_at: Option<SystemTime>,
    /// When the Pi accepted it; `None` while sending.
    pub accepted_at: Option<SystemTime>,
    /// For own messages, where it stands; `None` for received ones.
    pub status: Option<SendStatus>,
}

/// The connection to the Pi, as the UI shows it (client-runtime: Connection
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
    /// A user changed their profile (message-content: User-owned profiles).
    ProfileChanged {
        /// The user whose device sent it.
        user: UserId,
        /// The display name, if set.
        display_name: Option<String>,
        /// The "about" text, if set.
        about: Option<String>,
    },
    /// The admin changed the group settings (message-content: Group settings
    /// travel encrypted).
    GroupSettingsChanged {
        /// The group's display name, if set.
        name: Option<String>,
    },
    /// Messages up to this time expired on the Pi before this device fetched
    /// them (message-delivery: Expiry and gap notice).
    Gap {
        /// `accepted_at` of the newest expired message.
        newest_expired_at: SystemTime,
    },
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
    /// The Pi presented a TLS key that does not match the pin (client-runtime:
    /// Connection lifecycle). The core stops connecting until
    /// [`crate::Client::start`] is called again.
    ServerIdentityError,
    /// This device joined the group from a Welcome for the first time.
    JoinedGroup,
    /// The group's member devices changed (a commit was merged).
    MembersChanged,
    /// This device refused a commit or Welcome the Pi delivered: a leaf's
    /// certificate, its key or the commit-authority rule did not check out
    /// (group-membership: Device-level membership; Commit authority). It
    /// cannot follow the group until it is re-added.
    CommitRefused {
        /// Why, for display.
        reason: String,
    },
    /// The connection state changed.
    Connection(ConnectionState),
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
