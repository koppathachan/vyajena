//! An in-process server on a random local port with a temporary database and keys
//! and a manual clock, plus gRPC connections over pinned TLS that know their
//! TLS exporter value (server-core design D8).

use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};

use hyper_util::rt::TokioIo;
use protocol::wire_transport::{AUTH_TLS_EXPORTER_LABEL, AUTH_TLS_EXPORTER_LEN};
use server::blobs::BlobStore;
use server::runtime::{Bootstrap, ManualClock, Server, State};
use tokio::sync::oneshot;
use tonic::transport::{Channel, Endpoint};

use super::pb::admin_client::AdminClient;
use super::pb::blobs_client::BlobsClient;
use super::pb::devices_client::DevicesClient;
use super::pb::dms_client::DmsClient;
use super::pb::keys_client::KeysClient;
use super::pb::messaging_client::MessagingClient;
use super::push::RecordingPush;
use super::{Check, config_with, manual_clock, tls_client};

/// How a test server stores image blobs.
#[derive(Clone)]
pub enum Blobs {
    /// Not configured: images are off.
    Off,
    /// The filesystem backend over `<dir>/blobs`, which exists.
    Filesystem,
    /// The filesystem backend over `<dir>/blobs`, which does not exist, as when
    /// the drive is not mounted.
    MissingDirectory,
    /// This backend instead of the one the config names.
    Backend(Arc<dyn BlobStore>),
}

impl Blobs {
    /// The config text for it.
    fn config(&self) -> &'static str {
        match self {
            Self::Off | Self::Backend(_) => "",
            Self::Filesystem | Self::MissingDirectory => {
                "[blob_store]\nbackend = \"filesystem\"\npath = \"blobs\"\n"
            }
        }
    }
}

/// A running test server.
pub struct TestServer {
    /// Moves the server's time.
    pub clock: ManualClock,
    /// Where it listens.
    pub addr: SocketAddr,
    /// Its TLS pin.
    pub pin: [u8; 32],
    /// Its shared state (store, config, keys).
    pub state: State,
    /// The bootstrap invite code created at start.
    pub bootstrap_code: String,
    /// Every wake ping the server sent (it also logs them, as the logging
    /// provider does).
    pub pushes: RecordingPush,
    blobs: Blobs,
    stop: Option<oneshot::Sender<()>>,
    _dir: tempfile::TempDir,
}

impl TestServer {
    /// Starts a fresh server (admin `akhil@chat`) whose wake pings are
    /// recorded in [`TestServer::pushes`].
    pub async fn start() -> Self {
        Self::start_inner(true, Blobs::Off).await
    }

    /// [`TestServer::start`] with image blobs stored as `blobs` says.
    pub async fn start_with_blobs(blobs: Blobs) -> Self {
        Self::start_inner(true, blobs).await
    }

    /// Starts a fresh server with the default push provider (the logging
    /// one); [`TestServer::pushes`] stays empty.
    pub async fn start_with_default_push() -> Self {
        Self::start_inner(false, Blobs::Off).await
    }

    async fn start_inner(record: bool, blobs: Blobs) -> Self {
        let dir = tempfile::tempdir().check("temp dir");
        if matches!(blobs, Blobs::Filesystem) {
            std::fs::create_dir(dir.path().join("blobs")).check("blob directory");
        }
        let clock = manual_clock();
        let pushes = RecordingPush::default();
        let server = if record {
            Self::start_server(dir.path(), &blobs, &clock, &pushes).await
        } else {
            Server::start(
                config_with(dir.path(), blobs.config()),
                Arc::new(clock.clone()),
            )
            .await
        }
        .check("server start");
        let bootstrap_code = match server.bootstrap() {
            Bootstrap::Created(invite) => invite.invite.code.clone(),
            other => panic!("expected a bootstrap invite, got {other:?}"),
        };
        let addr = server.local_addr();
        let pin = server.state().keys.tls_spki_sha256();
        let state = server.state().clone();
        let (stop, stopped) = oneshot::channel::<()>();
        tokio::spawn(server.serve(async {
            let _ = stopped.await;
        }));
        Self {
            clock,
            addr,
            pin,
            state,
            bootstrap_code,
            pushes,
            blobs,
            stop: Some(stop),
            _dir: dir,
        }
    }

    async fn start_server(
        dir: &std::path::Path,
        blobs: &Blobs,
        clock: &ManualClock,
        pushes: &RecordingPush,
    ) -> Result<Server, server::runtime::StartError> {
        let config = config_with(dir, blobs.config());
        let clock = Arc::new(clock.clone());
        let pushes = Arc::new(pushes.clone());
        match blobs {
            Blobs::Backend(backend) => {
                Server::start_with_blobs(config, clock, pushes, Some(backend.clone())).await
            }
            _ => Server::start_with_push(config, clock, pushes).await,
        }
    }

    /// The directory the filesystem blob backend writes to.
    pub fn blob_dir(&self) -> std::path::PathBuf {
        self._dir.path().join("blobs")
    }

    /// Makes the blob directory, as mounting the drive does.
    pub fn mount_blob_dir(&self) {
        std::fs::create_dir(self.blob_dir()).check("blob directory");
    }

    /// Stops the server and starts a new one on the same database and keys
    /// (another port), as after a power cut. Connections to the old one are
    /// left to fail.
    pub async fn restart(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let server = Self::start_server(self._dir.path(), &self.blobs, &self.clock, &self.pushes)
            .await
            .check("server restart");
        self.addr = server.local_addr();
        self.state = server.state().clone();
        let (stop, stopped) = oneshot::channel::<()>();
        tokio::spawn(server.serve(async {
            let _ = stopped.await;
        }));
        self.stop = Some(stop);
    }

    /// A new pinned TLS connection.
    pub async fn connect(&self) -> Conn {
        Conn::open(self.addr, self.pin).await
    }

    /// The server's current time.
    pub fn clock_now(&self) -> std::time::SystemTime {
        server::runtime::Clock::now(&self.clock)
    }

    /// Runs the server's time-driven upkeep now (inactivity, expiry, dedup).
    pub async fn sweep(&self) {
        server::messaging::sweep(&self.state).await.check("sweep");
        server::blobs::sweep(&self.state).await.check("blob sweep");
    }

    /// Runs a read-only query on the server's database.
    pub async fn query<R, F>(&self, f: F) -> R
    where
        R: Send + 'static,
        F: FnOnce(&rusqlite::Connection) -> rusqlite::Result<R> + Send + 'static,
    {
        self.state
            .store
            .call(move |conn| Ok(f(conn)?))
            .await
            .check("database query")
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

/// One HTTP/2 connection to the server over pinned TLS.
#[derive(Clone)]
pub struct Conn {
    /// The gRPC channel (exactly one TLS connection; it does not reconnect).
    pub channel: Channel,
    /// This connection's TLS exporter value, as the server sees it.
    pub exporter: [u8; AUTH_TLS_EXPORTER_LEN],
}

impl Conn {
    /// Connects over pinned TLS and wraps the stream in a gRPC channel.
    pub async fn open(addr: SocketAddr, pin: [u8; 32]) -> Self {
        let tls = tls_client::connect_pinned(addr, pin)
            .await
            .check("pinned TLS connection");
        let exporter = tls
            .get_ref()
            .1
            .export_keying_material(
                [0u8; AUTH_TLS_EXPORTER_LEN],
                AUTH_TLS_EXPORTER_LABEL,
                Some(&[]),
            )
            .check("TLS exporter");
        let slot = Arc::new(Mutex::new(Some(tls)));
        let channel = Endpoint::from_static("http://vyajena2-server")
            .connect_with_connector(tower::service_fn(move |_| {
                let stream = slot.lock().unwrap_or_else(PoisonError::into_inner).take();
                async move {
                    stream
                        .map(TokioIo::new)
                        .ok_or_else(|| std::io::Error::other("test connection already used"))
                }
            }))
            .await
            .check("gRPC channel");
        Self { channel, exporter }
    }

    /// A `Devices` client.
    pub fn devices(&self) -> DevicesClient<Channel> {
        DevicesClient::new(self.channel.clone())
    }

    /// A `Keys` client.
    pub fn keys(&self) -> KeysClient<Channel> {
        KeysClient::new(self.channel.clone())
    }

    /// A `Messaging` client.
    pub fn messaging(&self) -> MessagingClient<Channel> {
        MessagingClient::new(self.channel.clone())
    }

    /// An `Admin` client.
    pub fn admin(&self) -> AdminClient<Channel> {
        AdminClient::new(self.channel.clone())
    }

    /// A `Dms` client.
    pub fn dms(&self) -> DmsClient<Channel> {
        DmsClient::new(self.channel.clone())
    }

    /// A `Blobs` client.
    pub fn blobs(&self) -> BlobsClient<Channel> {
        BlobsClient::new(self.channel.clone())
    }
}

/// `request` with `authorization: Bearer <token>`.
pub fn authed<T>(token: &str, message: T) -> tonic::Request<T> {
    let mut request = tonic::Request::new(message);
    let value = format!("Bearer {token}")
        .parse()
        .check("authorization metadata");
    request.metadata_mut().insert("authorization", value);
    request
}
