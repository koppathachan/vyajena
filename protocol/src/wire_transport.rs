//! wire-transport: versioning, session authentication, size limits, heartbeats.

use std::time::Duration;

/// The protocol version this contract defines, sent in `Hello.protocol_version`
/// and listed in `ServerInfo.supported_protocol_versions`
/// (Protocol version negotiation).
pub const PROTOCOL_VERSION: u32 = 1;

/// Prefix of the bytes a device auth key signs to authenticate:
/// `AUTH_CONTEXT || nonce || tls_exporter` (Session authentication).
pub const AUTH_CONTEXT: &[u8] = b"vyajena2-auth-v1";

/// TLS 1.3 exporter label (RFC 8446 section 7.5) for the `tls_exporter` value
/// in the auth signature, used with an empty context (Session authentication).
pub const AUTH_TLS_EXPORTER_LABEL: &[u8] = b"EXPORTER-vyajena2-auth-v1";

/// Length of the `tls_exporter` value in the auth signature
/// (Session authentication).
pub const AUTH_TLS_EXPORTER_LEN: usize = 32;

/// gRPC metadata key carrying the session token (Session authentication).
pub const AUTHORIZATION_METADATA_KEY: &str = "authorization";

/// Scheme prefix of the `authorization` value: `Bearer <session_token>`.
pub const AUTHORIZATION_BEARER_PREFIX: &str = "Bearer ";

/// Upper bound on a session token's lifetime (Session authentication).
pub const SESSION_TOKEN_MAX_LIFETIME: Duration = Duration::from_secs(60 * 60);

/// Length of a challenge nonce from `Devices.GetChallenge`
/// (Session authentication).
pub const CHALLENGE_NONCE_LEN: usize = 32;

/// A challenge nonce is valid this long after it is issued, and only once
/// (Session authentication).
pub const CHALLENGE_TTL: Duration = Duration::from_secs(60);

/// Maximum `Envelope.ciphertext` length of an `APPLICATION` envelope
/// (Size limits).
pub const MAX_APPLICATION_CIPHERTEXT_BYTES: usize = 65_536;

/// Maximum encoded size of a `COMMIT` envelope, including its `welcome`, or a
/// `PROPOSAL` envelope (Size limits).
pub const MAX_HANDSHAKE_ENVELOPE_BYTES: usize = 262_144;

/// A client sends a heartbeat at least this often while the stream is idle
/// (Heartbeats).
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(25);

/// The Pi closes a stream that has received nothing for this long (Heartbeats).
pub const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(90);
