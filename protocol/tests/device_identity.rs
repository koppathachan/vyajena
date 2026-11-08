//! External tests for `protocol::device_identity` (public API only).

use ed25519_dalek::SigningKey;
use protocol::device_identity::{
    CertificateError, DEVICE_CERT_CONTEXT, DEVICE_ID_LEN, device_certificate_signing_input,
    device_id, device_id_matches, sign_device_certificate, verify_device_certificate,
};
use sha2::{Digest, Sha256};

fn identity_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

/// Stand-in for an encoded `DeviceCertificate`: the check treats it as opaque.
const CERTIFICATE: &[u8] = b"\x0a\x20encoded DeviceCertificate bytes";

// spec: device-identity/federation-ready-identifiers
#[test]
fn device_id_is_the_first_16_bytes_of_sha256_of_the_auth_key() {
    let auth_key = identity_key(7).verifying_key().to_bytes();
    let id = device_id(&auth_key);
    assert_eq!(id.len(), DEVICE_ID_LEN);
    assert_eq!(id.as_slice(), &Sha256::digest(auth_key)[..16]);
}

// spec: device-identity/federation-ready-identifiers
#[test]
fn device_id_is_derived_not_assigned() {
    // Two derivations of the same key agree, different keys differ.
    let a = identity_key(1).verifying_key().to_bytes();
    let b = identity_key(2).verifying_key().to_bytes();
    assert_eq!(device_id(&a), device_id(&a));
    assert_ne!(device_id(&a), device_id(&b));
}

// spec: device-identity/federation-ready-identifiers
#[test]
fn device_id_claim_must_match_the_auth_key() {
    let auth_key = identity_key(3).verifying_key().to_bytes();
    let id = device_id(&auth_key);
    assert!(device_id_matches(&auth_key, &id));

    let mut wrong = id;
    wrong[0] ^= 1;
    assert!(!device_id_matches(&auth_key, &wrong));
    assert!(!device_id_matches(&auth_key, &id[..15]));
    assert!(!device_id_matches(&auth_key, &[]));
}

// spec: device-identity/user-identity-key-and-device-certificates
#[test]
fn signing_input_is_context_then_certificate_bytes() {
    let input = device_certificate_signing_input(CERTIFICATE);
    assert_eq!(&input[..DEVICE_CERT_CONTEXT.len()], DEVICE_CERT_CONTEXT);
    assert_eq!(&input[DEVICE_CERT_CONTEXT.len()..], CERTIFICATE);
}

// spec: device-identity/user-identity-key-and-device-certificates
#[test]
fn valid_certificate_verifies_against_its_identity_key() {
    let identity = identity_key(9);
    let signature = sign_device_certificate(&identity, CERTIFICATE);
    assert_eq!(
        verify_device_certificate(
            &identity.verifying_key().to_bytes(),
            CERTIFICATE,
            &signature
        ),
        Ok(())
    );
}

// spec: device-identity/user-identity-key-and-device-certificates
#[test]
fn certificate_signed_by_another_identity_key_is_forged() {
    let alice = identity_key(10);
    let mallory = identity_key(11);
    let signature = sign_device_certificate(&mallory, CERTIFICATE);
    assert_eq!(
        verify_device_certificate(&alice.verifying_key().to_bytes(), CERTIFICATE, &signature),
        Err(CertificateError::BadSignature)
    );
}

// spec: device-identity/user-identity-key-and-device-certificates
#[test]
fn tampered_certificate_bytes_are_rejected() {
    let identity = identity_key(12);
    let signature = sign_device_certificate(&identity, CERTIFICATE);
    let mut tampered = CERTIFICATE.to_vec();
    tampered[3] ^= 0x01;
    assert_eq!(
        verify_device_certificate(&identity.verifying_key().to_bytes(), &tampered, &signature),
        Err(CertificateError::BadSignature)
    );
}

// spec: device-identity/user-identity-key-and-device-certificates
#[test]
fn signature_without_the_context_string_is_rejected() {
    use ed25519_dalek::Signer;
    let identity = identity_key(13);
    // Signed over the bare certificate, without "vyajena2-device-cert-v1".
    let signature = identity.sign(CERTIFICATE).to_bytes();
    assert_eq!(
        verify_device_certificate(
            &identity.verifying_key().to_bytes(),
            CERTIFICATE,
            &signature
        ),
        Err(CertificateError::BadSignature)
    );
}

// spec: device-identity/user-identity-key-and-device-certificates
#[test]
fn malformed_keys_and_signatures_are_rejected() {
    let identity = identity_key(14);
    let public = identity.verifying_key().to_bytes();
    let signature = sign_device_certificate(&identity, CERTIFICATE);

    assert_eq!(
        verify_device_certificate(&public[..31], CERTIFICATE, &signature),
        Err(CertificateError::InvalidIdentityKey)
    );
    assert_eq!(
        verify_device_certificate(&public, CERTIFICATE, &signature[..63]),
        Err(CertificateError::InvalidSignatureLength)
    );
    // The identity point (a small-order key) is weak and never accepted.
    let mut weak = [0u8; 32];
    weak[0] = 1;
    assert_eq!(
        verify_device_certificate(&weak, CERTIFICATE, &signature),
        Err(CertificateError::InvalidIdentityKey)
    );
}
