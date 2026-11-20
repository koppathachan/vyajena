//! The TLS listener: TLS 1.3 only, HTTP/2 only, a self-signed certificate around
//! the server's TLS key (server-core design D6; wire-transport: gRPC over HTTP/2
//! and pinned TLS on TCP 443).
//!
//! Each accepted connection carries a [`TlsConnectInfo`] that tonic puts into
//! every request's extensions. It holds the connection's TLS exporter value
//! (label `EXPORTER-vyajena2-auth-v1`, empty context, 32 bytes), which
//! `Devices.Authenticate` checks the device's signature against
//! (wire-transport: Session authentication).

use std::fmt;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use protocol::wire_transport::{AUTH_TLS_EXPORTER_LABEL, AUTH_TLS_EXPORTER_LEN};
use rustls::ServerConfig;
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::server::TlsStream;
use tonic::transport::server::Connected;

use super::server_keys::ServerKeys;

/// Subject name in the self-signed certificate. Clients ignore it: they check
/// only the SPKI pin.
const CERT_SUBJECT: &str = "vyajena2-server";

/// A TLS handshake that takes longer than this is dropped.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Why the TLS setup failed.
#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    /// Building the self-signed certificate failed.
    #[error("building the TLS certificate failed: {0}")]
    Certificate(#[from] rcgen::Error),
    /// rustls refused the configuration.
    #[error("TLS configuration failed: {0}")]
    Config(#[from] rustls::Error),
}

/// The rustls server configuration for `keys`.
pub fn server_config(keys: &ServerKeys) -> Result<ServerConfig, TlsError> {
    let params = rcgen::CertificateParams::new(vec![CERT_SUBJECT.to_owned()])?;
    let cert = params.self_signed(keys.tls_key())?;
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(keys.tls_key().serialize_der()));

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_no_client_auth()
        .with_single_cert(vec![cert.der().clone()], key)?;
    config.alpn_protocols = vec![b"h2".to_vec()];
    Ok(config)
}

/// Per-connection facts tonic attaches to every request on it.
#[derive(Clone)]
pub struct TlsConnectInfo {
    /// The peer address as seen by the server (the relay, in production).
    pub remote_addr: SocketAddr,
    /// The TLS 1.3 exporter value the auth signature covers.
    pub tls_exporter: [u8; AUTH_TLS_EXPORTER_LEN],
}

impl fmt::Debug for TlsConnectInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsConnectInfo")
            .field("remote_addr", &self.remote_addr)
            .finish_non_exhaustive()
    }
}

/// An accepted TLS connection, ready for tonic.
pub struct TlsConnection {
    stream: TlsStream<TcpStream>,
    info: TlsConnectInfo,
}

impl Connected for TlsConnection {
    type ConnectInfo = TlsConnectInfo;

    fn connect_info(&self) -> Self::ConnectInfo {
        self.info.clone()
    }
}

impl AsyncRead for TlsConnection {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for TlsConnection {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.stream.is_write_vectored()
    }
}

/// Accepts TCP connections, runs each TLS handshake in its own task and hands
/// finished connections to tonic through the returned channel. Failed
/// handshakes are logged and dropped, so one bad peer never stops the server.
pub fn accept_loop(
    listener: TcpListener,
    config: ServerConfig,
) -> mpsc::Receiver<Result<TlsConnection, io::Error>> {
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(async move {
        loop {
            let accepted = tokio::select! {
                accepted = listener.accept() => accepted,
                // tonic dropped the receiver: the server is shutting down.
                () = tx.closed() => return,
            };
            let (tcp, remote_addr) = match accepted {
                Ok(accepted) => accepted,
                Err(error) => {
                    // Usually out of file descriptors; back off and retry.
                    tracing::warn!(%error, "accept failed");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            };
            let acceptor = acceptor.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                match handshake(&acceptor, tcp, remote_addr).await {
                    Ok(connection) => {
                        let _ = tx.send(Ok(connection)).await;
                    }
                    Err(error) => {
                        tracing::debug!(%remote_addr, %error, "TLS handshake failed");
                    }
                }
            });
        }
    });
    rx
}

async fn handshake(
    acceptor: &TlsAcceptor,
    tcp: TcpStream,
    remote_addr: SocketAddr,
) -> io::Result<TlsConnection> {
    let _ = tcp.set_nodelay(true);
    let stream = tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(tcp))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "TLS handshake timed out"))??;
    let tls_exporter = stream
        .get_ref()
        .1
        .export_keying_material(
            [0u8; AUTH_TLS_EXPORTER_LEN],
            AUTH_TLS_EXPORTER_LABEL,
            Some(&[]),
        )
        .map_err(io::Error::other)?;
    Ok(TlsConnection {
        stream,
        info: TlsConnectInfo {
            remote_addr,
            tls_exporter,
        },
    })
}
