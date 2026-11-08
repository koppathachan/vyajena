//! Rust code generated from the `vyajena.v1` protobuf contract under `proto/`,
//! plus the contract's limits, counts, timings and signing context strings.
//!
//! The `.proto` files are the source of truth (wire-transport: Protobuf contract
//! is the source of truth); this crate only re-exports what `tonic-prost-build`
//! generates from them. Each constant names the OpenSpec requirement it comes
//! from (change `core-wire-protocol`).

use std::time::Duration;

/// Generated messages, enums and gRPC clients/servers for package `vyajena.v1`.
#[allow(clippy::all, missing_docs)]
pub mod v1 {
    include!(concat!(env!("OUT_DIR"), "/vyajena.v1.rs"));
}

// --- Protocol version -------------------------------------------------------

/// The protocol version this contract defines, sent in `Hello.protocol_version`
/// and listed in `ServerInfo.supported_protocol_versions`
/// (wire-transport: Protocol version negotiation).
pub const PROTOCOL_VERSION: u32 = 1;

// --- Signing context strings ------------------------------------------------

/// Prefix of the bytes a user identity key signs for a device certificate:
/// `DEVICE_CERT_CONTEXT || SignedDeviceCertificate.certificate`
/// (device-identity: User identity key and device certificates; design D1, D4).
pub const DEVICE_CERT_CONTEXT: &[u8] = b"vyajena2-device-cert-v1";

/// Prefix of the bytes a device auth key signs to authenticate:
/// `AUTH_CONTEXT || nonce || tls_exporter` (wire-transport: Session
/// authentication; design D9).
pub const AUTH_CONTEXT: &[u8] = b"vyajena2-auth-v1";

/// TLS 1.3 exporter label (RFC 8446 section 7.5) for the `tls_exporter` value
/// in the auth signature, used with an empty context (wire-transport: Session
/// authentication; design D9).
pub const AUTH_TLS_EXPORTER_LABEL: &[u8] = b"EXPORTER-vyajena2-auth-v1";

/// Length of the `tls_exporter` value in the auth signature (wire-transport:
/// Session authentication).
pub const AUTH_TLS_EXPORTER_LEN: usize = 32;

// --- Identifiers and MLS ----------------------------------------------------

/// Length of a device id: the first 16 bytes of SHA-256(device auth public key)
/// (device-identity: Federation-ready identifiers).
pub const DEVICE_ID_LEN: usize = 16;

/// Length of a client-generated `Envelope.msg_id` (message-delivery: Envelope).
pub const MSG_ID_LEN: usize = 16;

/// The only MLS ciphersuite the Pi accepts (key-directory: KeyPackage upload).
pub const CIPHERSUITE_NAME: &str = "MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519";

/// RFC 9420 code point of [`CIPHERSUITE_NAME`].
pub const CIPHERSUITE_ID: u16 = 0x0001;

// --- Session authentication -------------------------------------------------

/// gRPC metadata key carrying the session token (wire-transport: Session
/// authentication).
pub const AUTHORIZATION_METADATA_KEY: &str = "authorization";

/// Scheme prefix of the `authorization` value: `Bearer <session_token>`
/// (design D9).
pub const AUTHORIZATION_BEARER_PREFIX: &str = "Bearer ";

/// Upper bound on a session token's lifetime (wire-transport: Session
/// authentication).
pub const SESSION_TOKEN_MAX_LIFETIME: Duration = Duration::from_secs(60 * 60);

/// Length of a challenge nonce: random bytes from `Devices.GetChallenge`
/// (wire-transport: Session authentication).
pub const CHALLENGE_NONCE_LEN: usize = 32;

/// A challenge nonce is valid this long after it is issued, and only once
/// (wire-transport: Session authentication).
pub const CHALLENGE_TTL: Duration = Duration::from_secs(60);

// --- Size limits ------------------------------------------------------------

/// Maximum `Envelope.ciphertext` length of an `APPLICATION` envelope
/// (wire-transport: Size limits; design D10).
pub const MAX_APPLICATION_CIPHERTEXT_BYTES: usize = 65_536;

/// Maximum encoded size of a `COMMIT` envelope, including its `welcome`, or a
/// `PROPOSAL` envelope (wire-transport: Size limits; message-delivery: Welcome
/// travels with its commit; design D10).
pub const MAX_HANDSHAKE_ENVELOPE_BYTES: usize = 262_144;

// --- KeyPackages ------------------------------------------------------------

/// One-time KeyPackages a device keeps on the Pi (key-directory: KeyPackage
/// upload; design D10).
pub const ONE_TIME_KEY_PACKAGES_TARGET: usize = 20;

/// The Pi sends `LowKeyPackages` when a device's one-time count drops below
/// this (key-directory: Replenishment signal; design D10).
pub const KEY_PACKAGES_LOW_WATER_MARK: usize = 5;

/// Last-resort KeyPackages per upload: exactly one (key-directory: KeyPackage
/// upload).
pub const LAST_RESORT_KEY_PACKAGES_PER_UPLOAD: usize = 1;

// --- Stream timings ---------------------------------------------------------

/// A client sends a heartbeat at least this often while the stream is idle
/// (wire-transport: Heartbeats).
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(25);

/// The Pi closes a stream that has received nothing for this long
/// (wire-transport: Heartbeats).
pub const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(90);

/// The Pi sends at most one wake ping per device per this interval
/// (message-delivery: Content-free wake pings).
pub const WAKE_PING_MIN_INTERVAL: Duration = Duration::from_secs(60);

// --- Queues, retention and inactivity ---------------------------------------

const DAY: u64 = 24 * 60 * 60;

/// Maximum application entries in one device's queue (message-delivery:
/// Per-device queues).
pub const QUEUE_CAP: usize = 1_000;

/// `APPLICATION` queue entries are deleted this long after `accepted_at`
/// (message-delivery: Expiry and gap notice).
pub const QUEUE_ENTRY_TTL: Duration = Duration::from_secs(7 * DAY);

/// The Pi deduplicates accepted envelopes on (sender device, `msg_id`) for at
/// least this long (message-delivery: Idempotent sends).
pub const DEDUP_MIN_RETENTION: Duration = Duration::from_secs(7 * DAY);

/// A member device that has not connected for this long is inactive
/// (group-membership: Inactive devices are kept out; design D7).
pub const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(7 * DAY);

/// An unused invite expires this long after creation (device-identity: Invites).
pub const INVITE_TTL: Duration = Duration::from_secs(7 * DAY);

#[cfg(test)]
mod tests {
    use super::v1;
    use prost::Message as _;

    #[test]
    fn signed_certificate_round_trips_as_opaque_bytes() {
        let cert = v1::DeviceCertificate {
            device: Some(v1::DeviceRef {
                user_id: Some(v1::UserId {
                    username: "alice".into(),
                    server_id: "pi".into(),
                }),
                device_id: vec![7; super::DEVICE_ID_LEN],
            }),
            auth_public_key: vec![1; 32],
            mls_signature_public_key: vec![2; 32],
            created_at: None,
        };
        let signed = v1::SignedDeviceCertificate {
            certificate: cert.encode_to_vec(),
            signature: vec![3; 64],
        };
        let decoded = v1::SignedDeviceCertificate::decode(&*signed.encode_to_vec()).unwrap();
        assert_eq!(decoded, signed);
        assert_eq!(v1::DeviceCertificate::decode(&*decoded.certificate).unwrap(), cert);
    }

    #[test]
    fn welcome_counts_toward_the_commit_envelope_cap() {
        // The Welcome travels inside its COMMIT envelope, and the handshake cap
        // applies to the whole encoded envelope (message-delivery: Welcome
        // travels with its commit; wire-transport: Size limits).
        let commit = v1::Envelope {
            msg_id: vec![1; super::MSG_ID_LEN],
            kind: v1::envelope::Kind::Commit as i32,
            ciphertext: vec![0; super::MAX_HANDSHAKE_ENVELOPE_BYTES / 2],
            welcome: Some(v1::WelcomeBundle {
                welcome: vec![0; super::MAX_HANDSHAKE_ENVELOPE_BYTES / 2],
                recipients: vec![],
            }),
            ..Default::default()
        };
        assert!(commit.encoded_len() > super::MAX_HANDSHAKE_ENVELOPE_BYTES);
        let decoded = v1::Envelope::decode(&*commit.encode_to_vec()).unwrap();
        assert_eq!(decoded, commit);
    }

    #[test]
    fn tls_exporter_label_uses_the_exporter_prefix() {
        // RFC 5705 / RFC 8446: application exporter labels start with "EXPORTER-".
        assert!(super::AUTH_TLS_EXPORTER_LABEL.starts_with(b"EXPORTER-"));
    }

    #[test]
    fn unknown_content_body_decodes_as_unset() {
        // A future body type (field 99) must decode, with `body` unset
        // (message-content: Inner content format).
        let mut bytes = v1::Content::default().encode_to_vec();
        bytes.extend_from_slice(&[0x9a, 0x06, 0x00]); // field 99, length-delimited, empty
        let content = v1::Content::decode(&*bytes).unwrap();
        assert!(content.body.is_none());
    }
}
