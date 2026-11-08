//! device-identity: identifiers, device certificates, invites.

use std::time::Duration;

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
