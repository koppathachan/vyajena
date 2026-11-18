//! The connection to the Pi (client-runtime: Connection lifecycle; client-core
//! task 4.2).
//!
//! - [`tls`]: TLS 1.3 to the relay address, accepting the Pi only by the
//!   SHA-256 pin of its SubjectPublicKeyInfo (wire-transport: gRPC over HTTP/2
//!   and pinned TLS on TCP 443).
//! - [`conn`]: one gRPC channel over one such connection, its TLS exporter
//!   value, challenge authentication and session tokens (wire-transport:
//!   Session authentication).
//! - [`backoff`]: reconnect delays.
//!
//! The `Connect` stream, heartbeats, token renewal and reconnecting are run by
//! the engine ([`crate::engine`]), which owns the connection.

pub mod backoff;
pub mod conn;
pub mod tls;

pub use backoff::Backoff;
pub use conn::{Conn, ResumeToken, Session};

/// Why talking to the Pi failed.
#[derive(Debug, Clone, thiserror::Error)]
pub enum NetError {
    /// The Pi presented a TLS key that does not match the pin: not the Pi
    /// from the invite (client-runtime: Connection lifecycle, "Wrong pin").
    #[error("the server's TLS key does not match the pinned key")]
    ServerIdentity,
    /// The TCP, TLS or HTTP/2 connection could not be made or broke.
    #[error("cannot reach the server: {0}")]
    Connect(String),
    /// The Pi answered an RPC with an error.
    #[error("the server refused: {0}")]
    Rpc(#[from] tonic::Status),
    /// The Pi's answer was not what the contract says.
    #[error("unexpected answer from the server: {0}")]
    Protocol(String),
}
