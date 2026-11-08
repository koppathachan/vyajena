//! pi-runtime: Durable storage. One SQLite database (bundled SQLite) in WAL mode
//! with `synchronous = FULL`, file mode 0600, and schema migrations embedded in
//! the binary and applied at start (pi-server-core design D5).
//!
//! All database work runs on one dedicated thread fed by a channel. That thread
//! is the only writer, which also gives design D4 its single writer: jobs run
//! one at a time, in the order they were sent, and a job that commits a
//! transaction has made it durable when its result comes back.

use std::fs::OpenOptions;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::mpsc as std_mpsc;
use std::thread;

use rusqlite::Connection;
use tokio::sync::oneshot;

mod migrations;

pub use migrations::SCHEMA_VERSION;

/// Mode of the database file (and, through SQLite, its `-wal` and `-shm`
/// files, which SQLite creates with the database file's mode).
pub const DATABASE_FILE_MODE: u32 = 0o600;

/// Why a database operation failed.
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
    /// The database was written by a newer server.
    #[error("database schema version {found} is newer than this server's {supported}")]
    NewerSchema {
        /// `PRAGMA user_version` in the file.
        found: i64,
        /// The newest version this binary knows.
        supported: i64,
    },
    /// SQLite did not switch to WAL mode.
    #[error("database refused WAL mode (journal_mode is {0})")]
    NotWal(String),
    /// The database thread has stopped.
    #[error("the database thread has stopped")]
    Closed,
    /// Stored data breaks an invariant the server relies on.
    #[error("inconsistent database: {0}")]
    Inconsistent(String),
}

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

/// A handle to the database thread. Cheap to clone; the thread stops when the
/// last handle is dropped.
#[derive(Clone, Debug)]
pub struct Store {
    jobs: std_mpsc::Sender<Job>,
}

impl Store {
    /// Opens (creating if needed) the database at `path`, applies migrations
    /// and starts the database thread.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        create_private(path)?;
        let (jobs, queue) = std_mpsc::channel::<Job>();
        let (ready_tx, ready_rx) = std_mpsc::sync_channel(1);
        let path = path.to_path_buf();
        thread::Builder::new()
            .name("sqlite-writer".into())
            .spawn(move || {
                let mut conn = match open_connection(&path) {
                    Ok(conn) => {
                        let _ = ready_tx.send(Ok(()));
                        conn
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                while let Ok(job) = queue.recv() {
                    job(&mut conn);
                }
            })
            .map_err(|source| StoreError::Io {
                path: PathBuf::from("sqlite-writer thread"),
                source,
            })?;
        ready_rx.recv().map_err(|_| StoreError::Closed)??;
        Ok(Self { jobs })
    }

    /// Runs `job` on the database thread and returns its result. Jobs run one
    /// at a time in submission order.
    pub async fn call<R, F>(&self, job: F) -> Result<R, StoreError>
    where
        R: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<R, StoreError> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        self.jobs
            .send(Box::new(move |conn| {
                let _ = tx.send(job(conn));
            }))
            .map_err(|_| StoreError::Closed)?;
        rx.await.map_err(|_| StoreError::Closed)?
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
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(DATABASE_FILE_MODE)
        .open(path)
        .map_err(io)?;
    file.set_permissions(std::fs::Permissions::from_mode(DATABASE_FILE_MODE))
        .map_err(io)
}

fn open_connection(path: &Path) -> Result<Connection, StoreError> {
    let mut conn = Connection::open(path)?;
    let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(StoreError::NotWal(mode));
    }
    conn.execute_batch(
        "PRAGMA synchronous = FULL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;",
    )?;
    migrations::apply(&mut conn)?;
    Ok(conn)
}
