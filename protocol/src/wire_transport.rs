//! wire-transport: versioning, session authentication, size limits, heartbeats.

use std::time::Duration;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

use crate::device_identity::{ED25519_PUBLIC_KEY_LEN, ED25519_SIGNATURE_LEN};

/// The protocol version this contract defines, sent in `Hello.protocol_version`
/// and listed in `ServerInfo.supported_protocol_versions`
/// (Protocol version negotiation). Version 2 adds DMs (change `dms`): a server that
/// lists it knows the `Dms` service. Version 3 adds named groups (change
/// `lobby-multi-group`). Version 4 adds manual presence (change
/// `manual-presence`): the server sends `PresenceUpdate` only to a stream that
/// negotiated 4 or above. On the stream all versions behave the same otherwise.
pub const PROTOCOL_VERSION: u32 = 4;

/// Every version the server accepts in `Hello`: version 1 is what 1.0.0 apps speak.
pub const SUPPORTED_PROTOCOL_VERSIONS: [u32; 4] = [1, 2, 3, 4];

/// manual-presence: the server sends `PresenceUpdate` only to a stream whose
/// `Hello.protocol_version` is at least this (change `manual-presence`, D7).
pub const PRESENCE_MIN_PROTOCOL_VERSION: u32 = 4;

/// The version a client speaks to a server that lists `supported`: the highest
/// one this contract knows, or `None` if they share none.
pub fn negotiate_protocol_version(supported: &[u32]) -> Option<u32> {
    SUPPORTED_PROTOCOL_VERSIONS
        .iter()
        .rev()
        .find(|v| supported.contains(v))
        .copied()
}

/// Whether a server that lists `supported` knows named groups (multi-group).
pub fn server_knows_named_groups(supported: &[u32]) -> bool {
    supported.contains(&3)
}

/// Whether a server that lists `supported` knows DMs (direct-messages).
pub fn server_knows_dms(supported: &[u32]) -> bool {
    supported.contains(&2)
}

/// Whether a server that lists `supported` knows manual presence (change
/// `manual-presence`): it accepts `SetPresence` and sends `PresenceUpdate`.
pub fn server_knows_presence(supported: &[u32]) -> bool {
    supported.contains(&4)
}

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

/// Upper bound on a session token's lifetime (Session authentication): six
/// hours, so a token saved in the key store survives an app being backgrounded
/// and reopened through a working day (change `resume-across-restart`).
pub const SESSION_TOKEN_MAX_LIFETIME: Duration = Duration::from_secs(6 * 60 * 60);

/// The largest request message the server decodes on any service but `Blobs`: well
/// above the 256 KiB handshake envelope, far below gRPC's 4 MiB default (change
/// `upload-limits`).
pub const MAX_REQUEST_MESSAGE_BYTES: usize = 1024 * 1024;

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

/// The server closes a stream that has received nothing for this long (Heartbeats).
pub const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(90);

/// The server sends an HTTP/2 keepalive PING on an otherwise-idle `Connect`
/// connection at least this often, so a dead or half-open connection is detected
/// well before [`STREAM_IDLE_TIMEOUT`] (Heartbeats). A conforming client's HTTP/2
/// layer answers PINGs automatically. Kept above [`HEARTBEAT_INTERVAL`] so the
/// server does not ping every idle connection just before each app heartbeat.
pub const HTTP2_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);

/// The server closes a `Connect` connection whose HTTP/2 keepalive PING is
/// unanswered for this long (Heartbeats). Detection is about
/// [`HTTP2_KEEPALIVE_INTERVAL`] + [`HTTP2_KEEPALIVE_TIMEOUT`]. Set with headroom
/// for a briefly-slow mobile radio, so a live client is not closed on a transient
/// stall.
pub const HTTP2_KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(20);

/// The exact bytes a device auth key signs to answer a challenge:
/// [`AUTH_CONTEXT`] `|| nonce || tls_exporter` (Session authentication).
pub fn auth_signing_input(nonce: &[u8], tls_exporter: &[u8]) -> Vec<u8> {
    let mut input = Vec::with_capacity(AUTH_CONTEXT.len() + nonce.len() + tls_exporter.len());
    input.extend_from_slice(AUTH_CONTEXT);
    input.extend_from_slice(nonce);
    input.extend_from_slice(tls_exporter);
    input
}

/// Signs a challenge with a device auth key, as the device core does
/// (Session authentication).
pub fn sign_auth_challenge(
    auth_key: &SigningKey,
    nonce: &[u8],
    tls_exporter: &[u8],
) -> [u8; ED25519_SIGNATURE_LEN] {
    auth_key
        .sign(&auth_signing_input(nonce, tls_exporter))
        .to_bytes()
}

/// Why an answer to a challenge was not accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AuthSignatureError {
    /// The device auth public key is not 32 bytes or not a valid Ed25519 point.
    #[error("device auth public key is not a valid Ed25519 key")]
    InvalidAuthKey,
    /// The nonce is not [`CHALLENGE_NONCE_LEN`] bytes or the exporter value is
    /// not [`AUTH_TLS_EXPORTER_LEN`] bytes.
    #[error("nonce or TLS exporter value has the wrong length")]
    InvalidInputLength,
    /// The signature is not 64 bytes.
    #[error("signature is not {ED25519_SIGNATURE_LEN} bytes")]
    InvalidSignatureLength,
    /// The signature does not verify.
    #[error("signature does not verify against the device auth key")]
    BadSignature,
}

/// Verifies `AuthenticateRequest.signature` over
/// `AUTH_CONTEXT || nonce || tls_exporter` against a device auth public key
/// (Session authentication). Strict Ed25519, as for device certificates.
pub fn verify_auth_signature(
    auth_public_key: &[u8],
    nonce: &[u8],
    tls_exporter: &[u8],
    signature: &[u8],
) -> Result<(), AuthSignatureError> {
    if nonce.len() != CHALLENGE_NONCE_LEN || tls_exporter.len() != AUTH_TLS_EXPORTER_LEN {
        return Err(AuthSignatureError::InvalidInputLength);
    }
    let key_bytes: &[u8; ED25519_PUBLIC_KEY_LEN] = auth_public_key
        .try_into()
        .map_err(|_| AuthSignatureError::InvalidAuthKey)?;
    let key =
        VerifyingKey::from_bytes(key_bytes).map_err(|_| AuthSignatureError::InvalidAuthKey)?;
    if key.is_weak() {
        return Err(AuthSignatureError::InvalidAuthKey);
    }
    let signature =
        Signature::from_slice(signature).map_err(|_| AuthSignatureError::InvalidSignatureLength)?;
    key.verify_strict(&auth_signing_input(nonce, tls_exporter), &signature)
        .map_err(|_| AuthSignatureError::BadSignature)
}
