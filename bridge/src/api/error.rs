//! What a failing bridge call throws in Dart.

/// Why a call failed, for the app to choose what to show. The core's
/// [`client::ClientError`] mapped one to one where the app cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreErrorKind {
    /// The key store failed or there is none.
    KeyStore,
    /// The local database failed.
    Storage,
    /// The server presented a TLS key that does not match the invite's pin
    /// (app-ui: Join from an invite, "server identity error").
    ServerIdentity,
    /// The server could not be reached, or answered something unexpected.
    Network,
    /// The server refused the call (for example a used invite, or an admin call
    /// from a non-admin device).
    Refused,
    /// The pasted invite has expired; a fresh one is needed (app-ui: Invite
    /// expiry is surfaced).
    InviteExpired,
    /// Text that is not an invite, link request or link approval, or one of
    /// the wrong kind.
    InvalidPayload,
    /// This device has no account yet.
    NotRegistered,
    /// This device already has an account.
    AlreadyRegistered,
    /// Not started or not connected right now.
    Offline,
    /// Only the admin's devices may do this.
    NotAdmin,
    /// This device does not hold the user identity key.
    NotIdentityDevice,
    /// `complete_link` without `link_request` first.
    NoLinkRequest,
    /// The server is not the one the invite or approval names.
    ServerMismatch,
    /// This device is not in the group.
    NoGroup,
    /// The group already exists.
    AlreadyInGroup,
    /// This device stopped following the group until it is re-added.
    Stalled,
    /// A user's identity key changed; accept it first.
    IdentityKeyChanged,
    /// A certificate or KeyPackage did not check out.
    Certificate,
    /// The server knows no such user.
    UnknownUser,
    /// The user has no KeyPackages yet.
    NoKeyPackages,
    /// The commit would be larger than the server accepts.
    TooLarge,
    /// The text is longer than a message may be (app-ui: Composer limit).
    TooLong,
    /// A mention does not fit the text (message-content: Mentions).
    InvalidMention,
    /// No stored chat message has that id (client-runtime: Reply lookup).
    NoSuchMessage,
    /// The server rejected the commit.
    Rejected,
    /// Not possible yet; the core retries.
    NotYet,
    /// No chat with that id (change `dms`).
    NoSuchChat,
    /// The DM is closed: its peer was removed.
    ChatClosed,
    /// The server does not support DMs yet.
    DmsUnavailable,
    /// The server does not support named groups yet.
    GroupsUnavailable,
    /// The picked file is not an image the app can send (JPEG, PNG, GIF or
    /// WebP).
    ImageUnsupported,
    /// The image cannot be made small enough to send.
    ImageTooLarge,
    /// The recording is not a WAV the app can send (16-bit PCM), or has no
    /// sound in it.
    AudioUnsupported,
    /// The voice message is longer than two minutes.
    AudioTooLong,
    /// The video is longer than the 30-second cap (video-messages).
    VideoTooLong,
    /// The video's encrypted blob is over the server's advertised maximum blob
    /// size (video-messages: Config-driven limit).
    VideoTooLarge,
    /// The bytes handed in are not an MP4 video the app can send
    /// (video-messages).
    NotAVideo,
    /// The server no longer has the picture or voice message (client-runtime:
    /// Expired media).
    MediaExpired,
    /// This device no longer has the picture to resend.
    MediaNotOnDevice,
    /// The one event stream was already taken.
    EventsTaken,
    /// Anything else (an internal failure of the core).
    Internal,
}

/// A failed call: its kind and the core's message for display. The message
/// never holds message text or keys.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct CoreError {
    /// What failed.
    pub kind: CoreErrorKind,
    /// The core's description.
    pub message: String,
}

impl CoreError {
    pub(crate) fn new(kind: CoreErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}
