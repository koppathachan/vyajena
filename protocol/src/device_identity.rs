//! device-identity: identifiers, device certificates, invites.

use std::time::Duration;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::DAY;

/// Length of a device id: the first 16 bytes of SHA-256(device auth public key)
/// (Federation-ready identifiers).
pub const DEVICE_ID_LEN: usize = 16;

/// Prefix of the bytes a user identity key signs for a device certificate:
/// `DEVICE_CERT_CONTEXT || SignedDeviceCertificate.certificate`
/// (User identity key and device certificates).
pub const DEVICE_CERT_CONTEXT: &[u8] = b"vyajena2-device-cert-v1";

/// An unused invite expires this long after creation (Invites).
pub const INVITE_TTL: Duration = Duration::from_secs(7 * DAY);

/// Length of an Ed25519 public key (user identity key, device auth key, MLS
/// signature key).
pub const ED25519_PUBLIC_KEY_LEN: usize = 32;

/// Length of an Ed25519 signature.
pub const ED25519_SIGNATURE_LEN: usize = 64;

/// A device id: the first 16 bytes of SHA-256 of the device auth public key.
pub type DeviceId = [u8; DEVICE_ID_LEN];

/// Derives a device id from its device auth public key
/// (Federation-ready identifiers). Any device can compute its own id; the server
/// never assigns one.
pub fn device_id(auth_public_key: &[u8]) -> DeviceId {
    let digest = Sha256::digest(auth_public_key);
    let mut id = [0u8; DEVICE_ID_LEN];
    id.copy_from_slice(&digest[..DEVICE_ID_LEN]);
    id
}

/// Whether `claimed` is the device id derived from `auth_public_key`. A claim of
/// the wrong length never matches.
pub fn device_id_matches(auth_public_key: &[u8], claimed: &[u8]) -> bool {
    claimed == device_id(auth_public_key).as_slice()
}

/// The exact bytes a user identity key signs for a device certificate:
/// [`DEVICE_CERT_CONTEXT`] followed by the encoded certificate.
pub fn device_certificate_signing_input(certificate: &[u8]) -> Vec<u8> {
    let mut input = Vec::with_capacity(DEVICE_CERT_CONTEXT.len() + certificate.len());
    input.extend_from_slice(DEVICE_CERT_CONTEXT);
    input.extend_from_slice(certificate);
    input
}

/// Signs encoded certificate bytes with a user identity key, as the identity
/// device does (User identity key and device certificates).
pub fn sign_device_certificate(
    identity_key: &SigningKey,
    certificate: &[u8],
) -> [u8; ED25519_SIGNATURE_LEN] {
    identity_key
        .sign(&device_certificate_signing_input(certificate))
        .to_bytes()
}

/// Why a device certificate signature was not accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CertificateError {
    /// The identity public key is not 32 bytes or not a valid Ed25519 point.
    #[error("identity public key is not a valid Ed25519 key")]
    InvalidIdentityKey,
    /// The signature is not 64 bytes.
    #[error("signature is not {ED25519_SIGNATURE_LEN} bytes")]
    InvalidSignatureLength,
    /// The signature does not verify against the identity key.
    #[error("signature does not verify against the identity key")]
    BadSignature,
}

/// Verifies `SignedDeviceCertificate.signature` over the carried
/// `certificate` bytes against a user identity public key
/// (User identity key and device certificates).
///
/// The bytes are checked as carried, never re-encoded; decode them as a
/// `DeviceCertificate` only after this returns `Ok`. Verification is strict
/// (RFC 8032 with the small-order and malleability checks), so a signature that
/// one side accepts the other side accepts too.
pub fn verify_device_certificate(
    identity_public_key: &[u8],
    certificate: &[u8],
    signature: &[u8],
) -> Result<(), CertificateError> {
    let key_bytes: &[u8; ED25519_PUBLIC_KEY_LEN] = identity_public_key
        .try_into()
        .map_err(|_| CertificateError::InvalidIdentityKey)?;
    let key =
        VerifyingKey::from_bytes(key_bytes).map_err(|_| CertificateError::InvalidIdentityKey)?;
    if key.is_weak() {
        return Err(CertificateError::InvalidIdentityKey);
    }
    let signature =
        Signature::from_slice(signature).map_err(|_| CertificateError::InvalidSignatureLength)?;
    key.verify_strict(&device_certificate_signing_input(certificate), &signature)
        .map_err(|_| CertificateError::BadSignature)
}

/// The longest push token the server accepts, in bytes (Push token registration).
pub const MAX_PUSH_TOKEN_BYTES: usize = 4096;

/// An APNs device token is this many hexadecimal characters, inclusive
/// (Push token registration; it goes into the APNs request path).
pub const APNS_TOKEN_HEX_LEN: std::ops::RangeInclusive<usize> = 64..=200;

/// Which platform a push token is for (the wire's `PushPlatform`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushTokenKind {
    /// Firebase Cloud Messaging.
    Fcm,
    /// Apple Push Notification service.
    Apns,
}

/// What is wrong with a push token, if anything (Push token registration):
/// empty, longer than [`MAX_PUSH_TOKEN_BYTES`], whitespace or control
/// characters, or for APNs not [`APNS_TOKEN_HEX_LEN`] hex characters.
pub fn push_token_problem(kind: PushTokenKind, token: &str) -> Option<&'static str> {
    if token.is_empty() {
        return Some("is empty");
    }
    if token.len() > MAX_PUSH_TOKEN_BYTES {
        return Some("is longer than 4096 bytes");
    }
    if token.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Some("contains whitespace or control characters");
    }
    if kind == PushTokenKind::Apns
        && !(APNS_TOKEN_HEX_LEN.contains(&token.len())
            && token.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Some("is not 64 to 200 hexadecimal characters");
    }
    None
}
