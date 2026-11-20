//! wire-transport: gRPC over HTTP/2 and pinned TLS on TCP 443. The client
//! accepts the server only when the SHA-256 hash of its TLS SubjectPublicKeyInfo
//! equals the pin from the invite. No certificate authority, name or validity
//! date is checked: the server's certificate is self-signed and the relay address
//! may be any name or IP.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use protocol::wire_transport::{AUTH_TLS_EXPORTER_LABEL, AUTH_TLS_EXPORTER_LEN};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

use super::NetError;

/// The TLS server name sent in SNI. The relay forwards raw TCP and the server
/// ignores it; it is fixed so the client never sends the relay's real name.
pub const TLS_SERVER_NAME: &str = "vyajena2-server";

/// The HTTP/2 ALPN id.
const ALPN_H2: &[u8] = b"h2";

/// Accepts exactly the server key whose SPKI hashes to `pin`, and records a
/// mismatch so the caller can tell it from a network error.
#[derive(Debug)]
struct PinVerifier {
    pin: [u8; 32],
    provider: Arc<CryptoProvider>,
    mismatch: Arc<AtomicBool>,
}

impl ServerCertVerifier for PinVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let matches = webpki::EndEntityCert::try_from(end_entity)
            .map(|cert| {
                Sha256::digest(cert.subject_public_key_info().as_ref()).as_slice() == self.pin
            })
            .unwrap_or(false);
        if matches {
            Ok(ServerCertVerified::assertion())
        } else {
            self.mismatch.store(true, Ordering::SeqCst);
            Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("TLS 1.2 is not allowed".into()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// A TLS connection to the server and its TLS exporter value.
pub struct PinnedStream {
    /// The connection.
    pub stream: TlsStream<TcpStream>,
    /// The TLS 1.3 exporter value with label `EXPORTER-vyajena2-auth-v1` and
    /// an empty context, which the auth signature covers.
    pub exporter: [u8; AUTH_TLS_EXPORTER_LEN],
}

/// Connects to `address` (`host:port`) over TLS 1.3 with ALPN `h2`,
/// accepting only the key pinned by `pin`. A mismatch is
/// [`NetError::ServerIdentity`]; nothing is sent over a connection to the
/// wrong key.
pub async fn connect(address: &str, pin: [u8; 32]) -> Result<PinnedStream, NetError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mismatch = Arc::new(AtomicBool::new(false));
    let mut config = ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| NetError::Connect(e.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinVerifier {
            pin,
            provider,
            mismatch: mismatch.clone(),
        }))
        .with_no_client_auth();
    config.alpn_protocols = vec![ALPN_H2.to_vec()];
    let tcp = TcpStream::connect(address)
        .await
        .map_err(|e| NetError::Connect(format!("{address}: {e}")))?;
    let name =
        ServerName::try_from(TLS_SERVER_NAME).map_err(|e| NetError::Connect(e.to_string()))?;
    let stream = match TlsConnector::from(Arc::new(config))
        .connect(name, tcp)
        .await
    {
        Ok(stream) => stream,
        Err(_) if mismatch.load(Ordering::SeqCst) => {
            tracing::warn!("the server's TLS key does not match the pin");
            return Err(NetError::ServerIdentity);
        }
        Err(e) => return Err(NetError::Connect(format!("TLS: {e}"))),
    };
    let exporter = stream
        .get_ref()
        .1
        .export_keying_material(
            [0u8; AUTH_TLS_EXPORTER_LEN],
            AUTH_TLS_EXPORTER_LABEL,
            Some(&[]),
        )
        .map_err(|e| NetError::Connect(format!("TLS exporter: {e}")))?;
    Ok(PinnedStream { stream, exporter })
}
