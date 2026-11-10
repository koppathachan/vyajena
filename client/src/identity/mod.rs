//! device-identity on the device: the user identity key (first device only),
//! the device auth and MLS signature keys, device certificates, and the
//! payloads a UI shows as a QR code or text (client-core task 4.1).
//!
//! - [`keys`]: the Ed25519 keys, kept only in the [`crate::keystore`].
//! - [`certificate`]: signing and checking `SignedDeviceCertificate`s with
//!   `protocol::device_identity`.
//! - [`payload`]: the invite payload, the link request and the link approval
//!   as bytes (the core produces and consumes bytes; the UI draws them).
//!
//! The flows themselves (`register`, `link_request`, `approve_link`,
//! `complete_link`, `revoke_device`, `set_push_token`) are on
//! [`crate::Client`].

pub mod certificate;
pub mod keys;
pub mod payload;

pub use certificate::{CertificateProblem, VerifiedCertificate};
pub use keys::DeviceKeys;
pub use payload::{Invite, LinkApproval, LinkRequest, PayloadError};
