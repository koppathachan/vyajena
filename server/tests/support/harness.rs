//! An in-process Pi on a random local port with a temporary database and keys
//! and a manual clock, plus gRPC connections over pinned TLS that know their
//! TLS exporter value (pi-server-core design D8).

use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};

use hyper_util::rt::TokioIo;
use protocol::wire_transport::{AUTH_TLS_EXPORTER_LABEL, AUTH_TLS_EXPORTER_LEN};
use server::runtime::{Bootstrap, ManualClock, Pi, State};
use tokio::sync::oneshot;
use tonic::transport::{Channel, Endpoint};

use super::pb::admin_client::AdminClient;
use super::pb::devices_client::DevicesClient;
use super::pb::keys_client::KeysClient;
use super::pb::messaging_client::MessagingClient;
use super::push::RecordingPush;
use super::{Check, config_in, manual_clock, tls_client};

/// A running test server.
pub struct TestPi {
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
    stop: Option<oneshot::Sender<()>>,
    _dir: tempfile::TempDir,
}

impl TestPi {
    /// Starts a fresh server (admin `akhil@pi`) whose wake pings are
    /// recorded in [`TestPi::pushes`].
    pub async fn start() -> Self {
        Self::start_inner(true).await
    }

    /// Starts a fresh server with the default push provider (the logging
    /// one); [`TestPi::pushes`] stays empty.
    pub async fn start_with_default_push() -> Self {
        Self::start_inner(false).await
    }

    async fn start_inner(record: bool) -> Self {
        let dir = tempfile::tempdir().check("temp dir");
        let clock = manual_clock();
        let pushes = RecordingPush::default();
        let pi = if record {
            Pi::start_with_push(
                config_in(dir.path()),
                Arc::new(clock.clone()),
                Arc::new(pushes.clone()),
            )
            .await
        } else {
            Pi::start(config_in(dir.path()), Arc::new(clock.clone())).await
        }
        .check("server start");
        let bootstrap_code = match pi.bootstrap() {
            Bootstrap::Created(invite) => invite.invite.code.clone(),
            other => panic!("expected a bootstrap invite, got {other:?}"),
        };
        let addr = pi.local_addr();
        let pin = pi.state().keys.tls_spki_sha256();
        let state = pi.state().clone();
        let (stop, stopped) = oneshot::channel::<()>();
        tokio::spawn(pi.serve(async {
            let _ = stopped.await;
        }));
        Self {
            clock,
            addr,
            pin,
            state,
            bootstrap_code,
            pushes,
            stop: Some(stop),
            _dir: dir,
        }
    }

    /// Stops the server and starts a new one on the same database and keys
    /// (another port), as after a power cut. Connections to the old one are
    /// left to fail.
    pub async fn restart(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let pi = Pi::start_with_push(
            config_in(self._dir.path()),
            Arc::new(self.clock.clone()),
            Arc::new(self.pushes.clone()),
        )
        .await
        .check("server restart");
        self.addr = pi.local_addr();
        self.state = pi.state().clone();
        let (stop, stopped) = oneshot::channel::<()>();
        tokio::spawn(pi.serve(async {
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

impl Drop for TestPi {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

/// One HTTP/2 connection to the Pi over pinned TLS.
#[derive(Clone)]
pub struct Conn {
    /// The gRPC channel (exactly one TLS connection; it does not reconnect).
    pub channel: Channel,
    /// This connection's TLS exporter value, as the Pi sees it.
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
        let channel = Endpoint::from_static("http://vyajena2-pi")
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
