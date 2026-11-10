//! The public [`Client`] (client-core design D2). Task groups 4 to 6 add its
//! async methods (`register`, `link_request`, `approve_link`, `create_group`,
//! `add_user`, `remove_user`, `send_text`, `set_profile`,
//! `set_group_settings`, ...); this group opens the store and the event
//! stream.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use crate::clock::{Clock, SystemClock};
use crate::events::{self, EventSender, EventStream};
use crate::keystore::{KeyStore, KeyStoreError, KeyStoreKind};
use crate::store::{Store, StoreError};

/// The database file inside [`ClientConfig::data_dir`].
pub const DATABASE_FILE: &str = "vyajena2.sqlite";

/// How to open a [`Client`].
#[derive(Clone)]
pub struct ClientConfig {
    /// Where the database lives.
    pub data_dir: PathBuf,
    /// Where the keys live: from [`crate::keystore::open_key_store`], or a
    /// [`crate::keystore::MemoryKeyStore`] in tests.
    pub key_store: Arc<dyn KeyStore>,
    /// The clock; [`SystemClock`] unless a test moves time itself (design D7).
    pub clock: Arc<dyn Clock>,
}

impl ClientConfig {
    /// A config on the system clock.
    pub fn new(data_dir: impl Into<PathBuf>, key_store: Arc<dyn KeyStore>) -> Self {
        Self {
            data_dir: data_dir.into(),
            key_store,
            clock: Arc::new(SystemClock),
        }
    }

    /// The same config on `clock`.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }
}

impl std::fmt::Debug for ClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientConfig")
            .field("data_dir", &self.data_dir)
            .field("key_store", &self.key_store.kind())
            .finish_non_exhaustive()
    }
}

/// Why a [`Client`] call failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ClientError {
    /// The key store failed, or there is none (see
    /// [`KeyStoreError::NoKeychain`]).
    #[error(transparent)]
    KeyStore(#[from] KeyStoreError),
    /// The local store failed.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A blocking task could not run.
    #[error("internal task failed: {0}")]
    Task(String),
}

/// The device core: one per device and data directory.
pub struct Client {
    store: Arc<Store>,
    key_store: Arc<dyn KeyStore>,
    clock: Arc<dyn Clock>,
    #[allow(dead_code)] // emits from task group 4 on
    events: EventSender,
    stream: Option<EventStream>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("key_store", &self.key_store.kind())
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Opens the client: the database in `data_dir` (created if needed) and
    /// the storage key in the key store (created on first start). The key
    /// store may block, e.g. on a macOS keychain prompt, so this runs off the
    /// async runtime.
    pub async fn open(config: ClientConfig) -> Result<Self, ClientError> {
        let ClientConfig {
            data_dir,
            key_store,
            clock,
        } = config;
        let keys = Arc::clone(&key_store);
        let store = tokio::task::spawn_blocking(move || {
            Store::open(&data_dir.join(DATABASE_FILE), keys.as_ref())
        })
        .await
        .map_err(|error| ClientError::Task(error.to_string()))??;
        let (events, stream) = events::channel();
        tracing::info!(key_store = ?key_store.kind(), "client opened");
        Ok(Self {
            store: Arc::new(store),
            key_store,
            clock,
            events,
            stream: Some(stream),
        })
    }

    /// The one ordered event stream (client-runtime: Events in order). There
    /// is only one: the first call gets it, later calls get `None`.
    pub fn take_events(&mut self) -> Option<EventStream> {
        self.stream.take()
    }

    /// Where this client's keys live.
    pub fn key_store_kind(&self) -> KeyStoreKind {
        self.key_store.kind()
    }

    /// The time on this client's clock.
    pub fn now(&self) -> SystemTime {
        self.clock.now()
    }

    /// The local store, for the feature modules.
    #[allow(dead_code)] // used from task group 4 on
    pub(crate) fn store(&self) -> &Store {
        &self.store
    }
}
