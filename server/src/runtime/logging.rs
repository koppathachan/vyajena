//! server-runtime (relay-sni-fix): the server's default log filter.
//!
//! On top of the env-or-`info` base it quiets rustls server-handshake warnings
//! (`rustls::server::hs`) — the benign "IP address presented as hostname"
//! notices from external scanners reaching the relay's public port. Pinned-key
//! TLS ignores SNI, so these are noise; real errors from that module and every
//! other target are unaffected (design D1).

use tracing_subscriber::EnvFilter;

/// The default log filter: `RUST_LOG` (or `info`) with rustls server-handshake
/// warnings dropped.
pub fn filter() -> EnvFilter {
    EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"))
        .add_directive(
            "rustls::server::hs=error"
                .parse()
                .expect("a valid tracing directive"),
        )
}
