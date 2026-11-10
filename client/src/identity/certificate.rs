//! device-identity: User identity key and device certificates, on the device.
//! Certificates are signed here by the user identity key and checked against a
//! user's pinned identity key whenever they appear in a leaf
//! (group-membership: Device-level membership).
//!
//! The signature is checked over the carried bytes with
//! `protocol::device_identity`; only then are they decoded.

use std::time::{SystemTime, UNIX_EPOCH};

use ed25519_dalek::SigningKey;
use openmls::prelude::{BasicCredential, Credential, CredentialWithKey};
use prost::Message as _;
use protocol::device_identity::{
    DeviceId, ED25519_PUBLIC_KEY_LEN, device_id, sign_device_certificate, verify_device_certificate,
};

use crate::events::UserId;
use crate::pb;

/// A certificate whose signature verified and whose fields are well formed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedCertificate {
    /// The certified user.
    pub user: UserId,
    /// The certified device, the hash of `auth_public_key`.
    pub device_id: DeviceId,
    /// The device auth public key.
    pub auth_public_key: Vec<u8>,
    /// The device's MLS signature public key.
    pub mls_signature_public_key: Vec<u8>,
}

/// Why a certificate was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CertificateProblem {
    /// The signature does not verify against the identity key.
    #[error("certificate signature does not verify against the user identity key")]
    Forged,
    /// The certificate does not decode or has malformed fields.
    #[error("malformed certificate: {0}")]
    Malformed(&'static str),
}

pub(crate) fn timestamp(time: SystemTime) -> prost_types::Timestamp {
    let since = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    prost_types::Timestamp {
        seconds: i64::try_from(since.as_secs()).unwrap_or(i64::MAX),
        nanos: i32::try_from(since.subsec_nanos()).unwrap_or(0),
    }
}

pub(crate) fn from_timestamp(ts: &prost_types::Timestamp) -> SystemTime {
    let secs = u64::try_from(ts.seconds).unwrap_or(0);
    let nanos = u32::try_from(ts.nanos).unwrap_or(0);
    UNIX_EPOCH + std::time::Duration::new(secs, nanos.min(999_999_999))
}

/// Encodes and signs a certificate for a device of `user` with the user
/// identity key (device-identity: User identity key and device certificates).
pub fn sign(
    identity: &SigningKey,
    user: &UserId,
    auth_public_key: &[u8],
    mls_signature_public_key: &[u8],
    created_at: SystemTime,
) -> pb::SignedDeviceCertificate {
    let certificate = pb::DeviceCertificate {
        device: Some(pb::DeviceRef {
            user_id: Some(user.to_pb()),
            device_id: device_id(auth_public_key).to_vec(),
        }),
        auth_public_key: auth_public_key.to_vec(),
        mls_signature_public_key: mls_signature_public_key.to_vec(),
        created_at: Some(timestamp(created_at)),
    }
    .encode_to_vec();
    let signature = sign_device_certificate(identity, &certificate).to_vec();
    pb::SignedDeviceCertificate {
        certificate,
        signature,
    }
}

/// Verifies `signed` against `identity_public_key`, then decodes and checks
/// its fields: a user id, a 16-byte device id equal to the hash of a 32-byte
/// auth key, and a 32-byte MLS signature key.
pub fn verify(
    signed: &pb::SignedDeviceCertificate,
    identity_public_key: &[u8],
) -> Result<VerifiedCertificate, CertificateProblem> {
    verify_device_certificate(identity_public_key, &signed.certificate, &signed.signature)
        .map_err(|_| CertificateProblem::Forged)?;
    decode(signed)
}

/// Decodes and checks the fields of a certificate without checking its
/// signature. Only for reading which user a leaf claims, to find the
/// identity key to verify it against.
pub fn decode(
    signed: &pb::SignedDeviceCertificate,
) -> Result<VerifiedCertificate, CertificateProblem> {
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
        user: UserId::from_pb(&user),
        device_id: device_id(&cert.auth_public_key),
        auth_public_key: cert.auth_public_key,
        mls_signature_public_key: cert.mls_signature_public_key,
    })
}

/// The `SignedDeviceCertificate` carried in an MLS basic credential's
/// identity (group-membership: Device-level membership), unchecked.
pub fn from_credential(credential: &Credential) -> Option<pb::SignedDeviceCertificate> {
    let basic = BasicCredential::try_from(credential.clone()).ok()?;
    pb::SignedDeviceCertificate::decode(basic.identity()).ok()
}

/// The MLS credential of a device: a basic credential whose identity is the
/// encoded signed certificate, with the device's MLS signature key.
pub fn credential_with_key(
    signed: &pb::SignedDeviceCertificate,
    mls_signature_public_key: &[u8],
) -> CredentialWithKey {
    CredentialWithKey {
        credential: BasicCredential::new(signed.encode_to_vec()).into(),
        signature_key: mls_signature_public_key.to_vec().into(),
    }
}
