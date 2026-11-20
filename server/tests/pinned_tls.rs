//! wire-transport: gRPC over HTTP/2 and pinned TLS on TCP 443, the server's side:
//! TLS 1.3 only, ALPN `h2`, terminated on the server with the key whose SPKI hash
//! is the pin. Checking the pin is the client's job; the test client does it
//! the way the core will. The port is configuration (443 behind the relay,
//! 8443 locally; server-runtime: Running locally).

mod support;

use support::harness::TestServer;
use support::{Check, pb, tls_client};

// spec: wire-transport/grpc-over-http-2-and-pinned-tls-on-tcp-443
#[tokio::test]
async fn pinned_key_matches_and_grpc_calls_proceed() {
    let server = TestServer::start().await;
    assert_eq!(server.pin, server.state.keys.tls_spki_sha256());
    // Conn::open only completes when the served SPKI hashes to the pin.
    let conn = server.connect().await;
    let info = conn
        .devices()
        .get_server_info(pb::GetServerInfoRequest {})
        .await
        .check("GetServerInfo over pinned TLS")
        .into_inner();
    assert_eq!(info.server_info.check("server info").server_id, "chat");
}

// spec: wire-transport/grpc-over-http-2-and-pinned-tls-on-tcp-443
#[tokio::test]
async fn pinned_key_mismatch_aborts_before_any_request() {
    let server = TestServer::start().await;
    let mut wrong = server.pin;
    wrong[31] ^= 0x80;
    let result = tls_client::connect_pinned(server.addr, wrong).await;
    let error = result.err().check("handshake with a wrong pin must fail");
    assert!(
        error.to_string().contains("pin"),
        "a server identity error, got: {error}"
    );
}

// spec: wire-transport/grpc-over-http-2-and-pinned-tls-on-tcp-443
#[tokio::test]
async fn server_refuses_tls_1_2() {
    let server = TestServer::start().await;
    let result =
        tls_client::connect_pinned_with(server.addr, server.pin, &[&rustls::version::TLS12]).await;
    // Refused by the server with a protocol_version alert, not by the client.
    let error = result.err().check("a TLS 1.2 handshake must fail");
    assert!(
        error.to_string().contains("ProtocolVersion"),
        "the server speaks TLS 1.3 only, got: {error}"
    );
    // The same client offering TLS 1.3 gets through, with ALPN h2.
    let tls = tls_client::connect_pinned(server.addr, server.pin)
        .await
        .check("TLS 1.3 connection");
    let (_, session) = tls.get_ref();
    assert_eq!(
        session.protocol_version(),
        Some(rustls::ProtocolVersion::TLSv1_3)
    );
    assert_eq!(session.alpn_protocol(), Some(&b"h2"[..]));
}
