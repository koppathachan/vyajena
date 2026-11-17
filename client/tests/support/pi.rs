//! The real Pi in-process for end-to-end tests (client-core design D8): a
//! temporary database and keys and a manual clock, behind a test relay whose
//! address the invites name. The Pi binds a port of its own choosing at every
//! start (no free port picked and released first, which a parallel test could
//! take), and the relay is pointed at it.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use prost::Message as _;
use server::push::{PushProvider, WakeOutcome};
use server::runtime::{Bootstrap, Clock as _, Config, ManualClock, Pi, State};
use tokio::sync::oneshot;

use super::relay::Relay;
use super::{Check, start_time};

/// The server id of test servers.
pub const SERVER_ID: &str = "pi";

/// How long a helper here waits for something it expects (the same as the
/// device helpers' `WAIT`).
pub const WAIT: Duration = Duration::from_secs(20);

/// The admin's username on test servers.
pub const ADMIN: &str = "akhil";

/// Records every wake ping the test server sends: only what the provider
/// interface carries, the device id and its token (message-delivery:
/// Content-free wake pings).
#[derive(Debug, Clone, Default)]
pub struct Pushes(Arc<Mutex<Vec<Ping>>>);

/// One recorded ping: the device id and its token.
type Ping = ([u8; 16], String);

impl Pushes {
    /// The pings so far, oldest first: device id and token.
    pub fn sent(&self) -> Vec<Ping> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Pings sent to `device` so far.
    pub fn count(&self, device: &[u8; 16]) -> usize {
        self.sent().iter().filter(|(id, _)| id == device).count()
    }
}

// What `#[async_trait]` expands to, written out: the bridge tests share this
// file and do not depend on tonic or async-trait.
impl PushProvider for Pushes {
    fn wake<'life0, 'async_trait>(
        &'life0 self,
        device_id: [u8; 16],
        token: server::pb::PushToken,
    ) -> Pin<Box<dyn Future<Output = WakeOutcome> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((device_id, token.token));
        Box::pin(std::future::ready(WakeOutcome::Sent))
    }
}

/// A running test server.
pub struct TestPi {
    /// Every wake ping it sent.
    pub pushes: Pushes,
    /// Moves the server's time.
    pub clock: ManualClock,
    /// Where it listens now (another port after a restart).
    pub addr: SocketAddr,
    /// The relay in front of it, whose address the invites name.
    pub relay: Relay,
    /// Its shared state.
    pub state: State,
    /// The bootstrap invite payload (encoded `InvitePayload`).
    pub bootstrap: Vec<u8>,
    stop: Option<oneshot::Sender<()>>,
    dir: tempfile::TempDir,
}

/// The Pi's config: any free local port, invites naming the relay.
fn config(dir: &std::path::Path, relay: SocketAddr) -> Config {
    let text = format!(
        r#"
server_id = "{SERVER_ID}"
admin_username = "{ADMIN}"
listen_address = "127.0.0.1:0"
public_address = "{relay}"
database_path = "pi.sqlite"
key_dir = "keys"
"#
    );
    Config::from_toml(&text, dir).check("test config")
}

async fn serve(pi: Pi) -> oneshot::Sender<()> {
    let (stop, stopped) = oneshot::channel::<()>();
    tokio::spawn(pi.serve(async {
        let _ = stopped.await;
    }));
    stop
}

impl TestPi {
    /// Starts a fresh server whose admin is `akhil@pi`.
    pub async fn start() -> Self {
        super::init_logging();
        let dir = tempfile::tempdir().check("temp dir");
        let clock = ManualClock::new(start_time());
        // The relay's own port is known before the Pi starts (its invites
        // name it); it learns the Pi's port once the Pi is listening.
        let relay = Relay::start(([127, 0, 0, 1], 0).into()).await;
        let pushes = Pushes::default();
        let pi = Pi::start_with_push(
            config(dir.path(), relay.addr),
            Arc::new(clock.clone()),
            Arc::new(pushes.clone()),
        )
        .await
        .check("server start");
        let addr = pi.local_addr();
        relay.retarget(addr);
        let bootstrap = match pi.bootstrap() {
            Bootstrap::Created(invite) => invite.payload.encode_to_vec(),
            other => panic!("expected a bootstrap invite, got {other:?}"),
        };
        let state = pi.state().clone();
        let stop = serve(pi).await;
        Self {
            pushes,
            clock,
            addr,
            relay,
            state,
            bootstrap,
            stop: Some(stop),
            dir,
        }
    }

    /// Stops the server. Open streams stay up until the relay is cut.
    pub async fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        // Let the listener close.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    /// Starts the server again on the same database and keys, behind the
    /// same relay address (on a new port of its own, which the relay is
    /// pointed at).
    pub async fn restart(&mut self) {
        self.stop().await;
        let pi = Pi::start_with_push(
            config(self.dir.path(), self.relay.addr),
            Arc::new(self.clock.clone()),
            Arc::new(self.pushes.clone()),
        )
        .await
        .check("server restart");
        self.addr = pi.local_addr();
        self.relay.retarget(self.addr);
        self.state = pi.state().clone();
        self.stop = Some(serve(pi).await);
    }

    /// Restarts the server with new TLS and external sender keys, as an
    /// impostor at the same address would present: devices' pins no longer
    /// match.
    pub async fn restart_with_new_keys(&mut self) {
        self.stop().await;
        std::fs::remove_dir_all(self.dir.path().join("keys")).check("remove keys");
        self.restart().await;
    }

    /// The server's current time.
    pub fn now(&self) -> SystemTime {
        self.clock.now()
    }

    /// Runs the server's time-driven upkeep now (inactivity, expiry).
    pub async fn sweep(&self) {
        server::messaging::sweep(&self.state).await.check("sweep");
    }

    /// Runs a query on the server's database.
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

    /// Whether `device` has an open `Connect` stream.
    pub fn is_connected(&self, device: &[u8; 16]) -> bool {
        self.state.hub.is_connected(device)
    }

    /// Waits until the Pi has registered `device`'s stream. The core reports
    /// Connected once the stream's response headers arrive; the Pi registers
    /// it only after reading its `Hello`, a moment later.
    pub async fn wait_connected(&self, device: &[u8; 16]) {
        self.wait_connection(device, true).await;
    }

    /// Waits until the Pi no longer has a stream for `device` (it
    /// unregisters a stream after the device has seen it end).
    pub async fn wait_disconnected(&self, device: &[u8; 16]) {
        self.wait_connection(device, false).await;
    }

    /// Waits until the Pi has closed the stream of `device` (stopped by the
    /// test) and recorded its last-seen time, which it writes right after
    /// unregistering the stream: a later clock jump is then not taken for
    /// time online.
    pub async fn wait_offline_recorded(&self, device: &[u8; 16]) {
        self.wait_disconnected(device).await;
        // The Pi reads its clock for the presence row after unregistering,
        // and the clock does not move while this waits.
        let now = self
            .now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| {
                i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
            });
        let id = *device;
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let seen: Option<i64> = self
                .query(move |db| {
                    db.query_row(
                        "SELECT max(last_seen_at) FROM presence WHERE device_id = ?1",
                        [id.as_slice()],
                        |row| row.get(0),
                    )
                })
                .await;
            if seen.is_some_and(|seen| seen >= now) {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the Pi did not record the device's last-seen time"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// The last-resort KeyPackage the Pi holds for `device`.
    pub async fn last_resort(&self, device: &[u8; 16]) -> Vec<u8> {
        let id = *device;
        self.query(move |db| {
            db.query_row(
                "SELECT key_package FROM key_packages WHERE device_id = ?1 AND last_resort = 1",
                [id.as_slice()],
                |row| row.get(0),
            )
        })
        .await
    }

    /// Waits until `device` has replaced its last-resort KeyPackage `first`.
    /// A device does that on the same timer check that finds its own leaf
    /// due for rotation (both are due a week after it joined), so this also
    /// tells a test that the device's timer has seen its clock.
    pub async fn wait_last_resort_replaced(&self, device: &[u8; 16], first: &[u8]) {
        let deadline = tokio::time::Instant::now() + WAIT;
        while self.last_resort(device).await == first {
            assert!(
                tokio::time::Instant::now() < deadline,
                "no last-resort refresh"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    async fn wait_connection(&self, device: &[u8; 16], connected: bool) {
        let deadline = tokio::time::Instant::now() + WAIT;
        while self.is_connected(device) != connected {
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for the Pi to see the stream {}",
                if connected { "open" } else { "closed" }
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

impl Drop for TestPi {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}
