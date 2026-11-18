//! wire-transport: Protocol version negotiation; Size limits (what
//! `Devices.GetServerInfo` advertises).

mod support;

use protocol::wire_transport::{
    MAX_APPLICATION_CIPHERTEXT_BYTES, MAX_HANDSHAKE_ENVELOPE_BYTES, SUPPORTED_PROTOCOL_VERSIONS,
};
use support::Check;
use support::harness::TestPi;
use support::pb;

// spec: wire-transport/protocol-version-negotiation
#[tokio::test]
async fn server_info_lists_id_versions_limits_and_external_sender() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    // No session token needed.
    let info = conn
        .devices()
        .get_server_info(pb::GetServerInfoRequest {})
        .await
        .check("GetServerInfo")
        .into_inner()
        .server_info
        .check("server_info");
    assert_eq!(info.server_id, "pi");
    assert_eq!(
        info.supported_protocol_versions,
        SUPPORTED_PROTOCOL_VERSIONS.to_vec()
    );
    let sender = info.external_sender.check("external_sender");
    assert_eq!(
        sender.signature_public_key,
        pi.state.keys.external_sender_public_key().to_vec()
    );
    // The credential is a TLS-serialized MLS Credential.
    let entry = support::clients::external_sender(&sender);
    assert_eq!(
        entry,
        server::membership::external_sender("pi", &pi.state.keys)
    );
}

// spec: wire-transport/size-limits
#[tokio::test]
async fn server_info_advertises_both_size_limits() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let limits = conn
        .devices()
        .get_server_info(pb::GetServerInfoRequest {})
        .await
        .check("GetServerInfo")
        .into_inner()
        .server_info
        .check("server_info")
        .limits
        .check("limits");
    assert_eq!(
        limits.max_application_ciphertext_bytes as usize,
        MAX_APPLICATION_CIPHERTEXT_BYTES
    );
    assert_eq!(
        limits.max_handshake_envelope_bytes as usize,
        MAX_HANDSHAKE_ENVELOPE_BYTES
    );
}
