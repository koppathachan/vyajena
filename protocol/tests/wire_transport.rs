//! External tests for `protocol::wire_transport` (public API only).

use protocol::wire_transport::{
    AUTH_TLS_EXPORTER_LABEL, MAX_APPLICATION_CIPHERTEXT_BYTES, MAX_HANDSHAKE_ENVELOPE_BYTES,
};

// spec: wire-transport/session-authentication
#[test]
fn tls_exporter_label_uses_the_exporter_prefix() {
    // RFC 5705 / RFC 8446: application exporter labels start with "EXPORTER-".
    assert!(AUTH_TLS_EXPORTER_LABEL.starts_with(b"EXPORTER-"));
}

// spec: wire-transport/size-limits
#[test]
fn size_caps_are_64_and_256_kib() {
    assert_eq!(MAX_APPLICATION_CIPHERTEXT_BYTES, 64 * 1024);
    assert_eq!(MAX_HANDSHAKE_ENVELOPE_BYTES, 256 * 1024);
}

mod auth_signature {
    use ed25519_dalek::SigningKey;
    use protocol::wire_transport::{
        AUTH_CONTEXT, AuthSignatureError, auth_signing_input, sign_auth_challenge,
        verify_auth_signature,
    };

    const NONCE: [u8; 32] = [7; 32];
    const EXPORTER: [u8; 32] = [9; 32];

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[42; 32])
    }

    // spec: wire-transport/session-authentication
    #[test]
    fn signing_input_is_context_nonce_exporter() {
        let input = auth_signing_input(&NONCE, &EXPORTER);
        assert_eq!(&input[..AUTH_CONTEXT.len()], b"vyajena2-auth-v1");
        assert_eq!(&input[AUTH_CONTEXT.len()..AUTH_CONTEXT.len() + 32], &NONCE);
        assert_eq!(&input[AUTH_CONTEXT.len() + 32..], &EXPORTER);
    }

    // spec: wire-transport/session-authentication
    #[test]
    fn a_signed_challenge_verifies() {
        let key = key();
        let signature = sign_auth_challenge(&key, &NONCE, &EXPORTER);
        assert_eq!(
            verify_auth_signature(
                &key.verifying_key().to_bytes(),
                &NONCE,
                &EXPORTER,
                &signature
            ),
            Ok(())
        );
    }

    // spec: wire-transport/session-authentication
    #[test]
    fn another_connections_exporter_does_not_verify() {
        let key = key();
        let signature = sign_auth_challenge(&key, &NONCE, &EXPORTER);
        let other = [10; 32];
        assert_eq!(
            verify_auth_signature(&key.verifying_key().to_bytes(), &NONCE, &other, &signature),
            Err(AuthSignatureError::BadSignature)
        );
    }

    // spec: wire-transport/session-authentication
    #[test]
    fn another_nonce_or_key_does_not_verify() {
        let key = key();
        let signature = sign_auth_challenge(&key, &NONCE, &EXPORTER);
        let public = key.verifying_key().to_bytes();
        assert_eq!(
            verify_auth_signature(&public, &[8; 32], &EXPORTER, &signature),
            Err(AuthSignatureError::BadSignature)
        );
        let stranger = SigningKey::from_bytes(&[43; 32]).verifying_key().to_bytes();
        assert_eq!(
            verify_auth_signature(&stranger, &NONCE, &EXPORTER, &signature),
            Err(AuthSignatureError::BadSignature)
        );
    }

    // spec: wire-transport/session-authentication
    #[test]
    fn wrong_lengths_are_refused() {
        let key = key();
        let public = key.verifying_key().to_bytes();
        let signature = sign_auth_challenge(&key, &NONCE, &EXPORTER);
        assert_eq!(
            verify_auth_signature(&public, &NONCE[..31], &EXPORTER, &signature),
            Err(AuthSignatureError::InvalidInputLength)
        );
        assert_eq!(
            verify_auth_signature(&public, &NONCE, &EXPORTER[..31], &signature),
            Err(AuthSignatureError::InvalidInputLength)
        );
        assert_eq!(
            verify_auth_signature(&public[..31], &NONCE, &EXPORTER, &signature),
            Err(AuthSignatureError::InvalidAuthKey)
        );
        assert_eq!(
            verify_auth_signature(&public, &NONCE, &EXPORTER, &signature[..63]),
            Err(AuthSignatureError::InvalidSignatureLength)
        );
    }
}
