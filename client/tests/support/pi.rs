//! The real Pi in-process for end-to-end tests (client-core design D8): a
//! temporary database and keys, a manual clock, and a fixed local port so it
//! can be restarted at the same relay address its invites name.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::SystemTime;

use prost::Message as _;
use server::runtime::{Bootstrap, Clock as _, Config, ManualClock, Pi, State};
use tokio::sync::oneshot;

use super::relay::Relay;
use super::{Check, start_time};

/// The server id of test servers.
pub const SERVER_ID: &str = "pi";

/// The admin's username on test servers.
pub const ADMIN: &str = "akhil";

/// A running test server.
pub struct TestPi {
    /// Moves the server's time.
    pub clock: ManualClock,
    /// Where it listens.
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

fn free_port() -> SocketAddr {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").check("bind a free port");
    listener.local_addr().check("local addr")
}

fn config(dir: &std::path::Path, addr: SocketAddr, relay: SocketAddr) -> Config {
    let text = format!(
        r#"
server_id = "{SERVER_ID}"
admin_username = "{ADMIN}"
listen_address = "{addr}"
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
        let addr = free_port();
        let relay = Relay::start(addr).await;
        let pi = Pi::start(
            config(dir.path(), addr, relay.addr),
            Arc::new(clock.clone()),
        )
        .await
        .check("server start");
        let bootstrap = match pi.bootstrap() {
            Bootstrap::Created(invite) => invite.payload.encode_to_vec(),
            other => panic!("expected a bootstrap invite, got {other:?}"),
        };
        let state = pi.state().clone();
        let stop = serve(pi).await;
        Self {
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

    /// Starts the server again on the same database, keys and port.
    pub async fn restart(&mut self) {
        self.stop().await;
        let mut tries = 0;
        let pi = loop {
            let config = config(self.dir.path(), self.addr, self.relay.addr);
            match Pi::start(config, Arc::new(self.clock.clone())).await {
                Ok(pi) => break pi,
                Err(error) if tries < 50 => {
                    tries += 1;
                    let _ = error;
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                Err(error) => panic!("server restart failed: {error:?}"),
            }
        };
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
}

impl Drop for TestPi {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}
