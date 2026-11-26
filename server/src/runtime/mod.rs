//! server-runtime: how the server starts and runs. [`Server::start`] loads or creates the
//! keys, opens the database, creates the bootstrap invite and binds the port;
//! [`server::serve`] runs the gRPC services over TLS until shutdown.

pub mod admin_reset;
pub mod bootstrap;
pub mod clock;
pub mod config;
pub mod logging;
pub mod redact;
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
use crate::blobs::BlobStore;
use crate::membership::readd::ReaddAsks;
use crate::messaging::Hub;
use crate::push::{LoggingProvider, PushProvider};
use crate::store::{Store, StoreError};

pub use admin_reset::{ResetError, ResetReport};
pub use bootstrap::{Bootstrap, BootstrapInvite};
pub use clock::{Clock, ManualClock, SystemClock};
pub use config::{Config, ConfigError, Onboarding, OnboardingMode, Recovery};
pub use redact::redact;
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
    /// The database belongs to a server with another server id; every stored user
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

/// Low-KeyPackage notices buffered for the messaging service.
const KEY_PACKAGES_LOW_CHANNEL_CAPACITY: usize = 256;

/// A device's one-time KeyPackage count dropped below the low water mark
/// (key-directory: Replenishment signal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyPackagesLow {
    /// The device.
    pub device_id: DeviceId,
    /// One-time KeyPackages left.
    pub remaining: u32,
}

/// What every service shares: config, keys, storage, the clock, and the
/// channels and registry the services use to reach each other's streams.
#[derive(Clone)]
pub struct State {
    /// The server's settings.
    pub config: Arc<Config>,
    /// The server's private keys.
    pub keys: Arc<ServerKeys>,
    /// The database.
    pub store: Store,
    /// The time source.
    pub clock: Arc<dyn Clock>,
    /// Ids of devices just revoked (`Devices.RevokeDevice`,
    /// `Admin.RemoveUser`), so the messaging service can close their open
    /// streams (device-identity: Device revocation).
    pub revocations: broadcast::Sender<DeviceId>,
    /// Devices whose one-time KeyPackages ran low at a fetch, so the
    /// messaging service can tell them (key-directory: Replenishment signal).
    pub key_packages_low: broadcast::Sender<KeyPackagesLow>,
    /// The open `Connect` streams (wire-transport: Single persistent
    /// messaging stream).
    pub hub: Arc<Hub>,
    /// Where wake pings go (server-runtime: Push provider interface).
    pub push: Arc<dyn PushProvider>,
    /// Recent `Keys.RequestReadd` asks per device (group-membership: Re-add
    /// after an unusable Welcome).
    pub readd_asks: Arc<ReaddAsks>,
    /// Where image blobs are kept; `None` when the server stores none
    /// (blob-store: Pluggable storage backend).
    pub blobs: Option<Arc<dyn BlobStore>>,
}

/// A started server: keys loaded, database open, port bound.
pub struct Server {
    state: State,
    listener: TcpListener,
    local_addr: SocketAddr,
    bootstrap: Bootstrap,
}

impl Server {
    /// Starts the server: keys, database, bootstrap invite, then the port. Logs
    /// the TLS pin and, when one is created, the bootstrap invite payload.
    pub async fn start(config: Config, clock: Arc<dyn Clock>) -> Result<Self, StartError> {
        Self::start_with_push(config, clock, Arc::new(LoggingProvider)).await
    }

    /// [`Server::start`] with a given push provider. Without one the server uses
    /// [`LoggingProvider`] (server-runtime: Push provider interface).
    pub async fn start_with_push(
        config: Config,
        clock: Arc<dyn Clock>,
        push: Arc<dyn PushProvider>,
    ) -> Result<Self, StartError> {
        let blobs = crate::blobs::backend_for(&config);
        Self::start_with_blobs(config, clock, push, blobs).await
    }

    /// [`Server::start_with_push`] with a given blob-store backend instead of the
    /// one the config names (blob-store: Pluggable storage backend).
    pub async fn start_with_blobs(
        config: Config,
        clock: Arc<dyn Clock>,
        push: Arc<dyn PushProvider>,
        blobs: Option<Arc<dyn BlobStore>>,
    ) -> Result<Self, StartError> {
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
        log_blob_store(blobs.as_deref()).await;

        let bootstrap = bootstrap::ensure_bootstrap_invite(
            &store,
            &config,
            keys.tls_spki_sha256(),
            clock.now(),
        )
        .await?;
        log_bootstrap(&config, &bootstrap);
        log_config(&config);

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
                key_packages_low: broadcast::channel(KEY_PACKAGES_LOW_CHANNEL_CAPACITY).0,
                hub: Arc::new(Hub::new()),
                push,
                readd_asks: Arc::new(ReaddAsks::default()),
                blobs,
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

    /// Serves gRPC over TLS until `shutdown` completes, then stops cleanly
    /// (server-runtime: Graceful stop): no new connections, open `Connect`
    /// streams end with `UNAVAILABLE` once the frame in progress is handled,
    /// requests in progress finish, and the database is closed.
    pub async fn serve(self, shutdown: impl Future<Output = ()>) -> Result<(), StartError> {
        let tls = tls::server_config(&self.state.keys)?;
        let incoming = ReceiverStream::new(tls::accept_loop(self.listener, tls));
        tracing::info!(listen_address = %self.local_addr, "listening (TLS 1.3, HTTP/2)");
        let auth = AuthLayer::new(
            self.state.store.clone(),
            self.state.config.clone(),
            self.state.clock.clone(),
        );
        let mut background = crate::messaging::background(&self.state);
        background.extend(crate::blobs::background(&self.state));
        let hub = self.state.hub.clone();
        let shutdown = async move {
            shutdown.await;
            hub.stop();
        };
        let served = tonic::transport::Server::builder()
            .layer(auth)
            .add_routes(routes(&self.state))
            .serve_with_incoming_shutdown(incoming, shutdown)
            .await;
        for task in background {
            task.abort();
        }
        // Every request has finished; jobs still queued run before the close.
        if let Err(error) = self.state.store.close().await {
            tracing::warn!(%error, "closing the database");
        }
        served?;
        Ok(())
    }
}

/// The gRPC services, one `add_service` per feature folder. [`server::serve`] puts
/// the [`AuthLayer`] in front of all of them (wire-transport: Session
/// authentication).
pub fn routes(state: &State) -> Routes {
    // Every request is decoded within a cap (blob-store: Upload limits): the
    // blobs service has its own, the rest share MAX_REQUEST_MESSAGE_BYTES.
    let cap = protocol::wire_transport::MAX_REQUEST_MESSAGE_BYTES;
    Routes::default()
        .add_service(crate::devices::service(state).max_decoding_message_size(cap))
        .add_service(crate::keys::service(state).max_decoding_message_size(cap))
        .add_service(crate::admin::service(state).max_decoding_message_size(cap))
        .add_service(crate::dms::service(state).max_decoding_message_size(cap))
        .add_service(crate::prefs::service(state).max_decoding_message_size(cap))
        .add_service(crate::blobs::service(state))
        .add_service(crate::messaging::service(state).max_decoding_message_size(cap))
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

/// Says at start whether images are on and usable. An unusable backend does
/// not stop the server: messaging runs and the `Blobs` RPCs answer
/// `UNAVAILABLE` until it is back (blob-store: Blob storage and graceful
/// degradation).
async fn log_blob_store(blobs: Option<&dyn BlobStore>) {
    match blobs {
        None => tracing::info!("no blob store configured: images are off"),
        Some(backend) => match backend.check().await {
            Ok(()) => tracing::info!("blob store ready"),
            Err(error) => tracing::warn!(
                %error,
                "blob store configured but unavailable: image RPCs answer UNAVAILABLE until it is back and the server restarts"
            ),
        },
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

/// Logs the effective, non-secret configuration at startup (server-runtime:
/// Effective configuration is logged at startup), so an operator can see what
/// is in force. Never logs keys, key-file contents or message content.
fn log_config(config: &Config) {
    let l = &config.limits;
    tracing::info!(
        server_id = %config.server_id,
        admin_username = %config.admin_username,
        // Not `listen_address`: the real bound address is logged separately as
        // that field, and the test harness scrapes it; avoid the collision.
        config_listen_address = %config.listen_address,
        public_address = %config.public_address,
        blob_store = config.blob_store.is_some(),
        inactivity_timeout_secs = l.inactivity_timeout.as_secs(),
        queue_entry_ttl_secs = l.queue_entry_ttl.as_secs(),
        queue_cap = l.queue_cap as u64,
        stream_idle_timeout_secs = l.stream_idle_timeout.as_secs(),
        invite_ttl_secs = l.invite_ttl.as_secs(),
        blob_ttl_secs = l.blob_ttl.as_secs(),
        challenge_ttl_secs = l.challenge_ttl.as_secs(),
        dedup_min_retention_secs = l.dedup_min_retention.as_secs(),
        wake_ping_min_interval_secs = l.wake_ping_min_interval.as_secs(),
        catch_up_max = l.catch_up_max as u64,
        sweep_interval_secs = l.sweep_interval.as_secs(),
        max_blob_store_bytes = l.max_blob_store_bytes,
        max_uploads_per_device_per_hour = l.max_uploads_per_device_per_hour as u64,
        // dev-onboarding: the mode and the non-secret onboarding settings. The
        // join_code value itself is NEVER logged, only whether one is required.
        onboarding_mode = ?config.onboarding.mode,
        onboarding_join_code_required = config.onboarding.join_code.is_some(),
        onboarding_max_users = config.onboarding.max_users.unwrap_or(0) as u64,
        "effective configuration"
    );
    if config.onboarding.mode == OnboardingMode::Open {
        tracing::warn!(
            "DEV onboarding is ENABLED (open mode): devices may self-register with no \
             invite, up to any configured max_users. This is for demos/testing only \
             and must not be used in production."
        );
    }
}
