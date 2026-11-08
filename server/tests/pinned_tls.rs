//! wire-transport: gRPC over HTTP/2 and pinned TLS on TCP 443, the Pi's side:
//! TLS 1.3 only, ALPN `h2`, terminated on the Pi with the key whose SPKI hash
//! is the pin. Checking the pin is the client's job; the test client does it
//! the way the core will. The port is configuration (443 behind the relay,
//! 8443 locally; pi-runtime: Running locally).

mod support;

use support::harness::TestPi;
use support::{Check, pb, tls_client};

// spec: wire-transport/grpc-over-http-2-and-pinned-tls-on-tcp-443
#[tokio::test]
async fn pinned_key_matches_and_grpc_calls_proceed() {
    let pi = TestPi::start().await;
    assert_eq!(pi.pin, pi.state.keys.tls_spki_sha256());
    // Conn::open only completes when the served SPKI hashes to the pin.
    let conn = pi.connect().await;
    let info = conn
        .devices()
        .get_server_info(pb::GetServerInfoRequest {})
        .await
        .check("GetServerInfo over pinned TLS")
        .into_inner();
    assert_eq!(info.server_info.check("server info").server_id, "pi");
}

// spec: wire-transport/grpc-over-http-2-and-pinned-tls-on-tcp-443
#[tokio::test]
async fn pinned_key_mismatch_aborts_before_any_request() {
    let pi = TestPi::start().await;
    let mut wrong = pi.pin;
    wrong[31] ^= 0x80;
    let result = tls_client::connect_pinned(pi.addr, wrong).await;
    let error = result.err().check("handshake with a wrong pin must fail");
    assert!(
        error.to_string().contains("pin"),
        "a server identity error, got: {error}"
    );
}

// spec: wire-transport/grpc-over-http-2-and-pinned-tls-on-tcp-443
#[tokio::test]
async fn pi_refuses_tls_1_2() {
    let pi = TestPi::start().await;
    let result = tls_client::connect_pinned_with(pi.addr, pi.pin, &[&rustls::version::TLS12]).await;
    // Refused by the Pi with a protocol_version alert, not by the client.
    let error = result.err().check("a TLS 1.2 handshake must fail");
    assert!(
        error.to_string().contains("ProtocolVersion"),
        "the Pi speaks TLS 1.3 only, got: {error}"
    );
    // The same client offering TLS 1.3 gets through, with ALPN h2.
    let tls = tls_client::connect_pinned(pi.addr, pi.pin)
        .await
        .check("TLS 1.3 connection");
    let (_, session) = tls.get_ref();
    assert_eq!(
        session.protocol_version(),
        Some(rustls::ProtocolVersion::TLSv1_3)
    );
    assert_eq!(session.alpn_protocol(), Some(&b"h2"[..]));
}
