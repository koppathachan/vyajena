//! wire-transport: Protocol version negotiation; Size limits (what
//! `Devices.GetServerInfo` advertises).

mod support;

use protocol::wire_transport::{
    MAX_APPLICATION_CIPHERTEXT_BYTES, MAX_HANDSHAKE_ENVELOPE_BYTES, SUPPORTED_PROTOCOL_VERSIONS,
};
use support::Check;
use support::harness::TestServer;
use support::pb;

// spec: wire-transport/protocol-version-negotiation
#[tokio::test]
async fn server_info_lists_id_versions_limits_and_external_sender() {
    let server = TestServer::start().await;
    let conn = server.connect().await;
    // No session token needed.
    let info = conn
        .devices()
        .get_server_info(pb::GetServerInfoRequest {})
        .await
        .check("GetServerInfo")
        .into_inner()
        .server_info
        .check("server_info");
    assert_eq!(info.server_id, "chat");
    assert_eq!(
        info.supported_protocol_versions,
        SUPPORTED_PROTOCOL_VERSIONS.to_vec()
    );
    let sender = info.external_sender.check("external_sender");
    assert_eq!(
        sender.signature_public_key,
        server.state.keys.external_sender_public_key().to_vec()
    );
    // The credential is a TLS-serialized MLS Credential.
    let entry = support::clients::external_sender(&sender);
    assert_eq!(
        entry,
        server::membership::external_sender("chat", &server.state.keys)
    );
}

// spec: wire-transport/size-limits
#[tokio::test]
async fn server_info_advertises_both_size_limits() {
    let server = TestServer::start().await;
    let conn = server.connect().await;
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
    // video-messages: the configured blob cap, the video default with no config.
    assert_eq!(
        limits.max_video_blob_bytes,
        protocol::video::DEFAULT_MAX_VIDEO_BYTES as u64
    );
}

// spec: wire-transport/size-limits
#[tokio::test]
async fn server_info_advertises_the_configured_video_blob_cap() {
    // The operator raises max_video_blob_bytes; GetServerInfo advertises the new
    // value (video-messages: Config-driven limit, "Operator raises the limit").
    let cap = protocol::video::DEFAULT_MAX_VIDEO_BYTES as u64 + 1_000_000;
    let extra = format!("[limits]\n{} = {cap}\n", protocol::video::CONFIG_KEY);
    let server = TestServer::start_with_blobs_and_config(support::harness::Blobs::Off, &extra).await;
    let conn = server.connect().await;
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
    assert_eq!(limits.max_video_blob_bytes, cap);
}

// spec: video-messages/config-driven-limit
#[tokio::test]
async fn a_config_above_the_ceiling_is_advertised_clamped() {
    // "Config beyond the ceiling": the server clamps to MAX_VIDEO_BYTES and
    // advertises the clamped value, rather than refusing to start.
    let extra = format!(
        "[limits]\n{} = {}\n",
        protocol::video::CONFIG_KEY,
        protocol::video::MAX_VIDEO_BYTES as u64 + 10_000_000
    );
    let server = TestServer::start_with_blobs_and_config(support::harness::Blobs::Off, &extra).await;
    let conn = server.connect().await;
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
        limits.max_video_blob_bytes,
        protocol::video::MAX_VIDEO_BYTES as u64
    );
}
