//! Fake FCM, Google OAuth and APNs servers for the push tests (push tasks
//! 1.4-1.6): HTTPS on loopback with a throwaway certificate for `localhost`,
//! HTTP/1.1 and HTTP/2, recording every request and answering from a closure.
//! Also throwaway keys: an RSA service account and an APNs P-256 key.

#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper_util::rt::{TokioExecutor, TokioIo};
use rcgen::{CertificateParams, KeyPair, PKCS_ECDSA_P256_SHA256};
use ring::signature::{ECDSA_P256_SHA256_FIXED, RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use server::push::Options;
use tokio::net::TcpListener;

/// One request the fake got.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub version: hyper::Version,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Recorded {
    /// A header's value (names are lower case).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// The body as JSON.
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or_default()
    }

    /// The body as text.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// What the fake answers.
#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Reply {
    pub fn new(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: body.to_owned(),
        }
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }
}

type Responder = Arc<dyn Fn(&Recorded) -> Reply + Send + Sync>;

/// A running fake server.
#[derive(Clone)]
pub struct FakeServer {
    /// `https://localhost:<port>`
    pub base: String,
    pub addr: SocketAddr,
    /// Its self-signed certificate, to trust in [`Options::extra_roots`].
    pub root: CertificateDer<'static>,
    requests: Arc<Mutex<Vec<Recorded>>>,
    responder: Arc<Mutex<Responder>>,
}

impl FakeServer {
    /// Starts a fake that answers `/token` like Google's OAuth endpoint
    /// (`access_token` "at-1", one hour) and everything else with 200 `{}`.
    pub async fn start() -> Self {
        let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("key");
        let cert = CertificateParams::new(vec!["localhost".to_owned()])
            .expect("params")
            .self_signed(&key)
            .expect("cert");
        let root = cert.der().clone();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut tls = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("versions")
            .with_no_client_auth()
            .with_single_cert(
                vec![root.clone()],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der())),
            )
            .expect("tls");
        tls.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let server = Self {
            base: format!("https://localhost:{}", addr.port()),
            addr,
            root,
            requests: Arc::default(),
            responder: Arc::new(Mutex::new(Arc::new(default_reply))),
        };
        let shared = server.clone();
        tokio::spawn(async move {
            loop {
                let Ok((tcp, _)) = listener.accept().await else {
                    continue;
                };
                let acceptor = acceptor.clone();
                let shared = shared.clone();
                tokio::spawn(async move {
                    let Ok(tls) = acceptor.accept(tcp).await else {
                        return;
                    };
                    let service = hyper::service::service_fn(move |req| {
                        let shared = shared.clone();
                        async move { Ok::<_, std::convert::Infallible>(shared.handle(req).await) }
                    });
                    let _ = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
                        .serve_connection(TokioIo::new(tls), service)
                        .await;
                });
            }
        });
        server
    }

    async fn handle(
        &self,
        req: hyper::Request<hyper::body::Incoming>,
    ) -> hyper::Response<Full<Bytes>> {
        let (parts, body) = req.into_parts();
        let body = body
            .collect()
            .await
            .map(|b| b.to_bytes().to_vec())
            .unwrap_or_default();
        let recorded = Recorded {
            method: parts.method.to_string(),
            path: parts.uri.path().to_owned(),
            version: parts.version,
            headers: parts
                .headers
                .iter()
                .map(|(n, v)| (n.as_str().to_owned(), v.to_str().unwrap_or("").to_owned()))
                .collect(),
            body,
        };
        let responder = self
            .responder
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let reply = responder(&recorded);
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(recorded);
        let mut response = hyper::Response::builder().status(reply.status);
        for (name, value) in &reply.headers {
            response = response.header(name, value);
        }
        response
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(reply.body)))
            .expect("response")
    }

    /// Answers every request with `f` from now on.
    pub fn respond(&self, f: impl Fn(&Recorded) -> Reply + Send + Sync + 'static) {
        *self
            .responder
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Arc::new(f);
    }

    /// Every request so far.
    pub fn requests(&self) -> Vec<Recorded> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Requests whose path starts with `prefix`.
    pub fn requests_to(&self, prefix: &str) -> Vec<Recorded> {
        self.requests()
            .into_iter()
            .filter(|r| r.path.starts_with(prefix))
            .collect()
    }
}

/// The default answers: OAuth on `/token`, 200 `{}` elsewhere.
pub fn default_reply(req: &Recorded) -> Reply {
    if req.path == "/token" {
        Reply::new(
            200,
            r#"{"access_token":"at-1","expires_in":3600,"token_type":"Bearer"}"#,
        )
    } else {
        Reply::new(200, "{}")
    }
}

/// Options for the providers pointed at fakes: FCM at `fcm`, APNs production
/// at `apns` and sandbox at `sandbox`, fast backoff.
pub fn options(fcm: &FakeServer, apns: &FakeServer, sandbox: &FakeServer) -> Options {
    Options {
        fcm_base: fcm.base.clone(),
        apns_production: apns.base.clone(),
        apns_sandbox: sandbox.base.clone(),
        proxy: None,
        extra_roots: vec![fcm.root.clone(), apns.root.clone(), sandbox.root.clone()],
        backoff_base: Duration::from_millis(50),
        max_retries: 3,
        max_wait: Duration::from_secs(5),
        request_timeout: Duration::from_secs(5),
    }
}

/// A throwaway RSA key (made once per test binary): PKCS#8 PEM and the
/// PKCS#1 public key ring verifies with.
pub fn rsa_key() -> &'static (String, Vec<u8>) {
    static KEY: OnceLock<(String, Vec<u8>)> = OnceLock::new();
    KEY.get_or_init(|| {
        use rsa::pkcs1::EncodeRsaPublicKey;
        use rsa::pkcs8::{EncodePrivateKey, LineEnding};
        let key = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).expect("rsa");
        let pem = key.to_pkcs8_pem(LineEnding::LF).expect("pem").to_string();
        let public = key.to_public_key().to_pkcs1_der().expect("der").to_vec();
        (pem, public)
    })
}

/// A service account key file's JSON for project `vyajena-test` whose
/// `token_uri` is the fake's `/token`.
pub fn service_account_json(oauth: &FakeServer) -> String {
    serde_json::json!({
        "type": "service_account",
        "project_id": "vyajena-test",
        "private_key_id": "kid-1",
        "private_key": rsa_key().0,
        "client_email": "vyajena-push@vyajena-test.iam.gserviceaccount.com",
        "client_id": "1",
        "token_uri": format!("{}/token", oauth.base),
    })
    .to_string()
}

/// A throwaway APNs key: PKCS#8 PEM and the raw public point.
pub fn apns_key() -> (String, Vec<u8>) {
    let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("p256");
    (key.serialize_pem(), key.public_key_raw().to_vec())
}

fn split_jwt(jwt: &str) -> (serde_json::Value, serde_json::Value, Vec<u8>, String) {
    let parts: Vec<&str> = jwt.split('.').collect();
    assert_eq!(parts.len(), 3, "JWT has three parts");
    let decode = |p: &str| URL_SAFE_NO_PAD.decode(p).expect("base64url");
    let header = serde_json::from_slice(&decode(parts[0])).expect("header");
    let claims = serde_json::from_slice(&decode(parts[1])).expect("claims");
    (
        header,
        claims,
        decode(parts[2]),
        format!("{}.{}", parts[0], parts[1]),
    )
}

/// Checks an RS256 JWT against `public` (PKCS#1); returns header and claims.
pub fn verify_rs256(jwt: &str, public: &[u8]) -> (serde_json::Value, serde_json::Value) {
    let (header, claims, signature, input) = split_jwt(jwt);
    UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, public)
        .verify(input.as_bytes(), &signature)
        .expect("RS256 signature verifies");
    (header, claims)
}

/// Checks an ES256 JWT against `public` (raw point); returns header and claims.
pub fn verify_es256(jwt: &str, public: &[u8]) -> (serde_json::Value, serde_json::Value) {
    let (header, claims, signature, input) = split_jwt(jwt);
    UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, public)
        .verify(input.as_bytes(), &signature)
        .expect("ES256 signature verifies");
    (header, claims)
}

/// The `assertion` of an OAuth JWT bearer form body.
pub fn assertion(form: &str) -> String {
    form.split('&')
        .find_map(|kv| kv.strip_prefix("assertion="))
        .expect("assertion")
        .to_owned()
}

/// A temp folder with mode 0700 for push credentials.
pub fn credentials_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    set_mode(dir.path(), 0o700);
    dir
}

/// Writes `name` in `dir` with mode 0600.
pub fn write_key(dir: &std::path::Path, name: &str, contents: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, contents).expect("write key");
    set_mode(&path, 0o600);
    path
}

pub fn set_mode(path: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
}

/// A 64-hex-character APNs token.
pub fn apns_token(n: u8) -> String {
    format!("{:02x}", n).repeat(32)
}
