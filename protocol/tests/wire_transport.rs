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
