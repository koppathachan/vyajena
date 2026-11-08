//! pi-runtime: how the Pi starts and runs. [`Pi::start`] loads or creates the
//! keys, opens the database, creates the bootstrap invite and binds the port;
//! [`Pi::serve`] runs the gRPC services over TLS until shutdown.

pub mod bootstrap;
pub mod clock;
pub mod config;
pub mod server_keys;
pub mod tls;

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

use protocol::device_identity::DeviceId;
use rusqlite::{OptionalExtension, params};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_stream::wrappers::ReceiverStream;
use tonic::service::Routes;

use crate::auth::AuthLayer;
use crate::store::{Store, StoreError};

pub use bootstrap::{Bootstrap, BootstrapInvite};
pub use clock::{Clock, ManualClock, SystemClock};
pub use config::{Config, ConfigError};
pub use server_keys::{KeyError, ServerKeys};
pub use tls::{TlsConnectInfo, TlsError};

/// Why the server could not start or stopped with an error.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// Loading or creating the keys failed.
    #[error("keys: {0}")]
    Keys(#[from] KeyError),
    /// Opening or using the database failed.
    #[error("database: {0}")]
    Store(#[from] StoreError),
    /// The database belongs to a Pi with another server id; every stored user
    /// id would change meaning.
    #[error(
        "config field `server_id` is {configured:?} but the database was created for {stored:?}"
    )]
    ServerIdMismatch {
        /// From the config.
        configured: String,
        /// From the database.
        stored: String,
    },
    /// Binding the listen address failed.
    #[error("cannot listen on {address}: {source}")]
    Bind {
        /// `listen_address`.
        address: SocketAddr,
        /// The I/O error.
        source: std::io::Error,
    },
    /// TLS setup failed.
    #[error("TLS: {0}")]
    Tls(#[from] TlsError),
    /// The gRPC server failed.
    #[error("gRPC server: {0}")]
    Serve(#[from] tonic::transport::Error),
}

/// Revocations buffered for slow subscribers before they lag.
const REVOCATION_CHANNEL_CAPACITY: usize = 256;

/// What every service shares: config, keys, storage and the clock.
#[derive(Clone)]
pub struct State {
    /// The server's settings.
    pub config: Arc<Config>,
    /// The Pi's private keys.
    pub keys: Arc<ServerKeys>,
    /// The database.
    pub store: Store,
    /// The time source.
    pub clock: Arc<dyn Clock>,
    /// Ids of devices just revoked (`Devices.RevokeDevice`,
    /// `Admin.RemoveUser`), so the messaging service can close their open
    /// streams (device-identity: Device revocation).
    pub revocations: broadcast::Sender<DeviceId>,
}

/// A started server: keys loaded, database open, port bound.
pub struct Pi {
    state: State,
    listener: TcpListener,
    local_addr: SocketAddr,
    bootstrap: Bootstrap,
}

impl Pi {
    /// Starts the server: keys, database, bootstrap invite, then the port. Logs
    /// the TLS pin and, when one is created, the bootstrap invite payload.
    pub async fn start(config: Config, clock: Arc<dyn Clock>) -> Result<Self, StartError> {
        let keys = ServerKeys::load_or_create(&config.key_dir)?;
        if keys.created() {
            tracing::info!(key_dir = %config.key_dir.display(), "first start: created the TLS and MLS external sender keys");
        }
        tracing::info!(
            tls_spki_sha256 = %keys.tls_spki_sha256_hex(),
            "TLS key pin (SHA-256 of the SubjectPublicKeyInfo)"
        );

        let store = Store::open(&config.database_path)?;
        check_server_id(&store, &config.server_id).await?;

        let bootstrap = bootstrap::ensure_bootstrap_invite(
            &store,
            &config,
            keys.tls_spki_sha256(),
            clock.now(),
        )
        .await?;
        log_bootstrap(&config, &bootstrap);

        let listener = TcpListener::bind(config.listen_address)
            .await
            .map_err(|source| StartError::Bind {
                address: config.listen_address,
                source,
            })?;
        let local_addr = listener.local_addr().map_err(|source| StartError::Bind {
            address: config.listen_address,
            source,
        })?;

        Ok(Self {
            state: State {
                config: Arc::new(config),
                keys: Arc::new(keys),
                store,
                clock,
                revocations: broadcast::channel(REVOCATION_CHANNEL_CAPACITY).0,
            },
            listener,
            local_addr,
            bootstrap,
        })
    }

    /// The address the server listens on (useful with port 0).
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// The shared state.
    pub fn state(&self) -> &State {
        &self.state
    }

    /// What the bootstrap step did at this start.
    pub fn bootstrap(&self) -> &Bootstrap {
        &self.bootstrap
    }

    /// Serves gRPC over TLS until `shutdown` completes.
    pub async fn serve(self, shutdown: impl Future<Output = ()>) -> Result<(), StartError> {
        let tls = tls::server_config(&self.state.keys)?;
        let incoming = ReceiverStream::new(tls::accept_loop(self.listener, tls));
        tracing::info!(listen_address = %self.local_addr, "listening (TLS 1.3, HTTP/2)");
        let auth = AuthLayer::new(
            self.state.store.clone(),
            self.state.config.clone(),
            self.state.clock.clone(),
        );
        tonic::transport::Server::builder()
            .layer(auth)
            .add_routes(routes(&self.state))
            .serve_with_incoming_shutdown(incoming, shutdown)
            .await?;
        Ok(())
    }
}

/// The gRPC services, one `add_service` per feature folder. [`Pi::serve`] puts
/// the [`AuthLayer`] in front of all of them (wire-transport: Session
/// authentication).
pub fn routes(state: &State) -> Routes {
    Routes::default()
        .add_service(crate::devices::service(state))
        .add_service(crate::keys::service(state))
        .add_service(crate::admin::service(state))
}

/// Records the server id on first start and refuses a different one later.
async fn check_server_id(store: &Store, configured: &str) -> Result<(), StartError> {
    let wanted = configured.to_owned();
    let stored = store
        .call(move |conn| {
            let tx = conn.transaction()?;
            let stored: Option<String> = tx
                .query_row(
                    "SELECT value FROM meta WHERE key = 'server_id'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            if stored.is_none() {
                tx.execute(
                    "INSERT INTO meta (key, value) VALUES ('server_id', ?1)",
                    params![wanted],
                )?;
                tx.commit()?;
            }
            Ok(stored)
        })
        .await?;
    match stored {
        Some(stored) if stored != configured => Err(StartError::ServerIdMismatch {
            configured: configured.to_owned(),
            stored,
        }),
        _ => Ok(()),
    }
}

fn log_bootstrap(config: &Config, bootstrap: &Bootstrap) {
    match bootstrap {
        Bootstrap::Created(invite) => tracing::info!(
            user_id = %config.admin_user_id(),
            invite_code = %invite.invite.code,
            relay_address = %invite.payload.relay_address,
            expires_at_unix_ms = clock::unix_millis(invite.invite.expires_at),
            payload = %invite.payload_text(),
            "bootstrap admin invite created; register the admin's first device with it"
        ),
        Bootstrap::Pending { expires_at } => tracing::info!(
            user_id = %config.admin_user_id(),
            expires_at_unix_ms = clock::unix_millis(*expires_at),
            "no users yet; the bootstrap invite printed at first start is still valid"
        ),
        Bootstrap::NotNeeded => {}
    }
}
