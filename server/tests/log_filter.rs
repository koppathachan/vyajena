//! server-runtime (relay-sni-fix): the server's default log filter quiets benign rustls
//! server-handshake warnings (external scanner IP-in-SNI noise).

// spec: server-runtime/quiet-external-tls-handshake-noise
#[test]
fn default_filter_quiets_rustls_handshake_warnings() {
    let filter = server::runtime::logging::filter().to_string();
    assert!(
        filter.contains("rustls::server::hs=error"),
        "filter is missing the rustls suppression: {filter}"
    );
    // The base level is still present (nothing else is suppressed).
    assert!(
        filter.contains("info") || std::env::var("RUST_LOG").is_ok(),
        "filter dropped the base level: {filter}"
    );
}
