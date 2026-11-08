//! device-identity: User identity key and device certificates. Reading and
//! checking `SignedDeviceCertificate`s, on their own or inside an MLS basic
//! credential (group-membership: Device-level membership).
//!
//! The signature is checked over the carried bytes with
//! `protocol::device_identity`; only then are they decoded.

use openmls::prelude::{BasicCredential, Credential};
use prost::Message as _;
use protocol::device_identity::{
    DeviceId, ED25519_PUBLIC_KEY_LEN, device_id, verify_device_certificate,
};
use tonic::Status;

use crate::pb;

/// A certificate whose signature verified and whose fields are well formed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedCertificate {
    /// The certified user's username.
    pub username: String,
    /// The certified user's server id.
    pub server_id: String,
    /// The certified device id, equal to the hash of `auth_public_key`.
    pub device_id: DeviceId,
    /// The device auth public key.
    pub auth_public_key: Vec<u8>,
    /// The device's MLS signature public key.
    pub mls_signature_public_key: Vec<u8>,
    /// The signed bytes, as carried.
    pub certificate: Vec<u8>,
    /// The identity key's signature over them.
    pub signature: Vec<u8>,
}

impl VerifiedCertificate {
    /// The certified user id, `<username>@<server_id>`.
    pub fn user_id(&self) -> String {
        format!("{}@{}", self.username, self.server_id)
    }
}

/// Why a certificate was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CertificateProblem {
    /// The signature does not verify against the identity key
    /// (`PERMISSION_DENIED`).
    #[error("certificate signature does not verify against the user identity key")]
    Forged,
    /// The certificate is absent, does not decode or has malformed fields
    /// (`INVALID_ARGUMENT`).
    #[error("malformed certificate: {0}")]
    Malformed(&'static str),
}

impl From<CertificateProblem> for Status {
    fn from(problem: CertificateProblem) -> Self {
        match problem {
            CertificateProblem::Forged => Status::permission_denied(problem.to_string()),
            CertificateProblem::Malformed(_) => Status::invalid_argument(problem.to_string()),
        }
    }
}

/// Verifies `signed` against `identity_public_key`, then decodes and checks its
/// fields: a user id, a 16-byte device id equal to the hash of a 32-byte auth
/// key, and a 32-byte MLS signature key.
pub fn verify(
    signed: &pb::SignedDeviceCertificate,
    identity_public_key: &[u8],
) -> Result<VerifiedCertificate, CertificateProblem> {
    verify_device_certificate(identity_public_key, &signed.certificate, &signed.signature)
        .map_err(|_| CertificateProblem::Forged)?;
    let cert = pb::DeviceCertificate::decode(signed.certificate.as_slice())
        .map_err(|_| CertificateProblem::Malformed("certificate bytes do not decode"))?;
    let device = cert
        .device
        .ok_or(CertificateProblem::Malformed("certificate names no device"))?;
    let user = device
        .user_id
        .ok_or(CertificateProblem::Malformed("certificate names no user"))?;
    if cert.auth_public_key.len() != ED25519_PUBLIC_KEY_LEN
        || cert.mls_signature_public_key.len() != ED25519_PUBLIC_KEY_LEN
    {
        return Err(CertificateProblem::Malformed(
            "certificate keys must be 32 bytes",
        ));
    }
    if device.device_id != device_id(&cert.auth_public_key) {
        return Err(CertificateProblem::Malformed(
            "device id is not derived from the auth key",
        ));
    }
    Ok(VerifiedCertificate {
        username: user.username,
        server_id: user.server_id,
        device_id: device_id(&cert.auth_public_key),
        auth_public_key: cert.auth_public_key,
        mls_signature_public_key: cert.mls_signature_public_key,
        certificate: signed.certificate.clone(),
        signature: signed.signature.clone(),
    })
}

/// The `SignedDeviceCertificate` carried in an MLS basic credential's
/// identity, without checking it.
pub fn from_credential(credential: &Credential) -> Option<pb::SignedDeviceCertificate> {
    let basic = BasicCredential::try_from(credential.clone()).ok()?;
    pb::SignedDeviceCertificate::decode(basic.identity()).ok()
}

/// The device id a signed certificate names, without checking the signature.
/// Used to find which stored device (and so which identity key) to check
/// against.
pub fn claimed_device_id(signed: &pb::SignedDeviceCertificate) -> Option<DeviceId> {
    let cert = pb::DeviceCertificate::decode(signed.certificate.as_slice()).ok()?;
    cert.device?.device_id.as_slice().try_into().ok()
}
