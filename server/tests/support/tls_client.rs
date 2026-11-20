//! A TLS client that authenticates the server only by its SPKI pin, as real clients
//! do (wire-transport: gRPC over HTTP/2 and pinned TLS on TCP 443).

use std::net::SocketAddr;
use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

/// Accepts exactly the server key whose SPKI hashes to `pin`.
#[derive(Debug)]
struct PinVerifier {
    pin: [u8; 32],
    provider: Arc<CryptoProvider>,
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
        let cert = webpki::EndEntityCert::try_from(end_entity)
            .map_err(|e| rustls::Error::General(format!("bad certificate: {e}")))?;
        let spki = cert.subject_public_key_info();
        if Sha256::digest(spki.as_ref()).as_slice() == self.pin {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General("TLS pin mismatch".into()))
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

/// Connects to `addr` over TLS 1.3 with ALPN `h2`, accepting only `pin`.
pub async fn connect_pinned(
    addr: SocketAddr,
    pin: [u8; 32],
) -> Result<TlsStream<TcpStream>, std::io::Error> {
    connect_pinned_with(addr, pin, &[&rustls::version::TLS13]).await
}

/// Like [`connect_pinned`], offering only the given TLS `versions` (for
/// negative tests).
pub async fn connect_pinned_with(
    addr: SocketAddr,
    pin: [u8; 32],
    versions: &[&'static rustls::SupportedProtocolVersion],
) -> Result<TlsStream<TcpStream>, std::io::Error> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(versions)
        .map_err(std::io::Error::other)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinVerifier { pin, provider }))
        .with_no_client_auth();
    config.alpn_protocols = vec![b"h2".to_vec()];
    let tcp = TcpStream::connect(addr).await?;
    let name = ServerName::try_from("vyajena2-server").map_err(std::io::Error::other)?;
    TlsConnector::from(Arc::new(config))
        .connect(name, tcp)
        .await
}
