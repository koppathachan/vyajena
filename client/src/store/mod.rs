//! The local SQLite store (client-runtime: Local message store; Key storage).
//!
//! One database file (bundled SQLite, WAL, `synchronous = FULL`, mode 0600 on
//! Unix) holds:
//!
//! - messages, with the plaintext of own messages and their send status
//!   ([`Store::insert_outgoing`] and friends);
//! - pinned identity keys ([`Store::pin_identity_key`]);
//! - this device's account, group bookkeeping, KeyPackage records and the own
//!   commit in flight ([`Store::account`] and friends);
//! - the OpenMLS group state, sealed under the storage key that lives in the
//!   key store ([`Store::mls`], design D3). Messages are not encrypted.
//!
//! The message store and OpenMLS share one connection, so
//! [`Store::transaction`] makes "process an MLS message, store the result"
//! atomic: the core acks to the server only after it commits (design D4).
//!
//! The store is synchronous and quick; it is used from one task at a time.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::ReentrantMutex;
use rusqlite::Connection;

use crate::keystore::{KeyName, KeyStore, KeyStoreError, get_or_create};
use crate::mls::{EncryptedStorage, MlsProvider, MlsStorageError, STORAGE_KEY_LEN};

mod blocked;
mod content_state;
mod device_state;
mod dms;
mod identity_keys;
mod media;
mod messages;
mod migrations;
mod prefs;
mod presence;

pub use content_state::ProfileRow;
pub use device_state::{AccountRow, GroupRow, OutgoingCommitRow};
pub use dms::{DmRow, MAIN_CHAT};
pub use identity_keys::{PinOutcome, PinnedIdentityKey};
pub use media::MediaEntry;
pub use messages::{IncomingMessage, InsertOutcome, OutgoingMessage, StoredMessage};
pub use migrations::SCHEMA_VERSION;

/// Mode of the database file on Unix (SQLite gives its `-wal` and `-shm`
/// files the same mode).
pub const DATABASE_FILE_MODE: u32 = 0o600;

/// Why a store operation failed.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// SQLite reported an error.
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Creating or checking the database file failed.
    #[error("{path}: {source}")]
    Io {
        /// The database file.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// The database was written by a newer version of the core.
    #[error("database schema version {found} is newer than this version's {supported}")]
    NewerSchema {
        /// `PRAGMA user_version` in the file.
        found: i64,
        /// The newest version this library knows.
        supported: i64,
    },
    /// SQLite did not switch to WAL mode.
    #[error("database refused WAL mode (journal_mode is {0})")]
    NotWal(String),
    /// The key store failed while reading or creating the storage key.
    #[error("key store: {0}")]
    KeyStore(#[from] KeyStoreError),
    /// The storage key in the key store has the wrong length.
    #[error("the storage key in the key store is not {STORAGE_KEY_LEN} bytes")]
    BadStorageKey,
    /// The OpenMLS storage failed, e.g. the storage key does not match the
    /// one the database was written with.
    #[error("OpenMLS storage: {0}")]
    Mls(#[from] MlsStorageError),
    /// A stored row breaks an invariant the store relies on.
    #[error("inconsistent database: {0}")]
    Inconsistent(String),
}

/// The connection shared by the message store and OpenMLS. Re-entrant, so a
/// [`Store::transaction`] can hold it while OpenMLS and the store run their own
/// statements on the same thread; other threads wait.
#[derive(Clone)]
pub(crate) struct Db(Arc<ReentrantMutex<RefCell<Connection>>>);

impl Db {
    fn new(conn: Connection) -> Self {
        Self(Arc::new(ReentrantMutex::new(RefCell::new(conn))))
    }

    /// Runs `f` on the connection.
    pub(crate) fn with<R>(&self, f: impl FnOnce(&Connection) -> R) -> R {
        let guard = self.0.lock();
        let conn = guard.borrow();
        f(&conn)
    }

    /// Runs `f` inside a savepoint: released if `f` succeeds, rolled back if it
    /// fails or panics. Nests.
    fn transaction<T, E>(&self, f: impl FnOnce() -> Result<T, E>) -> Result<T, E>
    where
        E: From<StoreError>,
    {
        let guard = self.0.lock();
        guard
            .borrow()
            .execute_batch("SAVEPOINT store_tx")
            .map_err(|error| E::from(StoreError::from(error)))?;
        let mut rollback = Rollback {
            db: self,
            armed: true,
        };
        let result = f();
        rollback.armed = false;
        let end = match &result {
            Ok(_) => "RELEASE store_tx",
            Err(_) => "ROLLBACK TO store_tx; RELEASE store_tx",
        };
        guard
            .borrow()
            .execute_batch(end)
            .map_err(|error| E::from(StoreError::from(error)))?;
        result
    }
}

/// Rolls an open savepoint back if `f` panics.
struct Rollback<'a> {
    db: &'a Db,
    armed: bool,
}

impl Drop for Rollback<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = self
                .db
                .with(|conn| conn.execute_batch("ROLLBACK TO store_tx; RELEASE store_tx"));
        }
    }
}

/// The local store of one device.
pub struct Store {
    db: Db,
    mls: MlsProvider,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").finish_non_exhaustive()
    }
}

impl Store {
    /// Opens (creating if needed) the database at `path`, applies migrations,
    /// and reads the storage key from `keys`, creating it on first start.
    /// Fails with [`MlsStorageError::WrongKey`] (as [`StoreError::Mls`]) if the
    /// database was written under another storage key.
    pub fn open(path: &Path, keys: &dyn KeyStore) -> Result<Self, StoreError> {
        create_private(path)?;
        let mut conn = Connection::open(path)?;
        let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(StoreError::NotWal(mode));
        }
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrations::apply(&mut conn)?;

        let secret = get_or_create(keys, KeyName::StorageKey, STORAGE_KEY_LEN)?;
        let storage_key: &[u8; STORAGE_KEY_LEN] = secret
            .as_slice()
            .try_into()
            .map_err(|_| StoreError::BadStorageKey)?;
        let db = Db::new(conn);
        let mls = MlsProvider::new(EncryptedStorage::new(db.clone(), storage_key)?);
        Ok(Self { db, mls })
    }

    /// The OpenMLS provider whose storage is this database.
    pub fn mls(&self) -> &MlsProvider {
        &self.mls
    }

    /// Runs `f` as one transaction: everything `f` writes through this store
    /// and through [`Store::mls`] commits together, or not at all if `f`
    /// returns an error or panics. After a rollback, reload any `MlsGroup`
    /// from storage: its in-memory state may be ahead of the database.
    pub fn transaction<T, E>(&self, f: impl FnOnce() -> Result<T, E>) -> Result<T, E>
    where
        E: From<StoreError>,
    {
        self.db.transaction(f)
    }

    /// Deletes every row from every table, returning the database to its freshly
    /// opened, empty state (account-deletion: The initiating device wipes itself
    /// on success; client-runtime: Device reset). The schema (its migration
    /// version) and the storage key are kept, so the same open database is
    /// reused for a later signup; only the data goes — the account, groups, DMs,
    /// message history, pending jobs and the OpenMLS group state. After it
    /// [`Store::account`] is `None`.
    pub fn wipe(&self) -> Result<(), StoreError> {
        self.db.with(|conn| {
            let tables: Vec<String> = {
                let mut stmt = conn.prepare(
                    "SELECT name FROM sqlite_master
                     WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
                )?;
                let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };
            // Drop foreign-key enforcement for the wipe so table order never
            // matters; it is re-enabled afterwards (it is a no-op inside a
            // transaction, and `with` hands out the connection in autocommit).
            conn.pragma_update(None, "foreign_keys", "OFF")?;
            let result = (|| {
                for table in &tables {
                    conn.execute(&format!("DELETE FROM \"{table}\""), [])?;
                }
                Ok::<(), StoreError>(())
            })();
            conn.pragma_update(None, "foreign_keys", "ON")?;
            result
        })
    }
}

/// Creates the database file with mode 0600 before SQLite opens it, and
/// tightens an existing file's mode.
fn create_private(path: &Path) -> Result<(), StoreError> {
    let io = |source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    };
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(DATABASE_FILE_MODE)
            .open(path)
            .map_err(io)?;
        file.set_permissions(std::fs::Permissions::from_mode(DATABASE_FILE_MODE))
            .map_err(io)?;
    }
    Ok(())
}
