//! blob-store: Pluggable storage backend. The seam between the `Blobs` service
//! and wherever the ciphertext bytes live.
//!
//! A backend stores and returns opaque bytes by blob id and nothing else. The
//! metadata, who may read a blob and when it dies all stay in the Pi's SQLite
//! (see [`super::meta`]), so a new backend (an object-storage bucket, other cheap
//! media) changes neither the wire protocol nor those rules. The MVP ships one,
//! [`FilesystemStore`].

use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Write as _};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use protocol::blob_store::BLOB_ID_LEN;

/// A blob id: [`BLOB_ID_LEN`] random bytes chosen by the sender.
pub type BlobId = [u8; BLOB_ID_LEN];

/// File mode of a stored blob.
const BLOB_FILE_MODE: u32 = 0o600;

/// The backend cannot be used right now: its drive is not mounted, its path is
/// not writable, or a remote store cannot be reached. The `Blobs` RPCs answer
/// `UNAVAILABLE` and messaging goes on (blob-store: Blob storage and graceful
/// degradation).
#[derive(Debug, thiserror::Error)]
#[error("the blob store is unavailable: {0}")]
pub struct BackendError(pub String);

impl From<std::io::Error> for BackendError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

/// Where blob ciphertext is kept.
#[tonic::async_trait]
pub trait BlobStore: Send + Sync + 'static {
    /// Whether the backend can be written to and read from now. Every `Blobs`
    /// RPC checks it first, so an absent backend answers `UNAVAILABLE` and never
    /// `NOT_FOUND` (blob-store: Blob storage and graceful degradation).
    async fn check(&self) -> Result<(), BackendError>;

    /// Stores `bytes` under `id`, replacing any bytes already there. Once it
    /// returns, the bytes are durable.
    async fn put(&self, id: &BlobId, bytes: Vec<u8>) -> Result<(), BackendError>;

    /// The bytes stored under `id`, or `None` if there are none.
    async fn get(&self, id: &BlobId) -> Result<Option<Vec<u8>>, BackendError>;

    /// Deletes the bytes under `id`. Deleting what is not there succeeds.
    async fn delete(&self, id: &BlobId) -> Result<(), BackendError>;

    /// Whether bytes are stored under `id`.
    async fn exists(&self, id: &BlobId) -> Result<bool, BackendError>;
}

/// Blobs as files in one directory, one file per blob named by its hex id.
///
/// The directory is the stable path the owner's auto-mounted drive provides. The
/// store never creates it: a missing directory means the drive is not there,
/// and creating one would put blobs on the main storage. An unmounted
/// mountpoint belongs to root, so the service user cannot write to it and the
/// probe in [`BlobStore::check`] fails.
#[derive(Debug, Clone)]
pub struct FilesystemStore {
    dir: PathBuf,
}

impl FilesystemStore {
    /// A store over `dir`. Nothing is touched until the first call.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// The directory blobs are kept in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn random_suffix() -> Result<String, BackendError> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).map_err(|e| BackendError(format!("no randomness: {e}")))?;
    Ok(hex(&bytes))
}

fn create_private(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(BLOB_FILE_MODE)
        .open(path)
}

/// Runs blocking file work off the async threads.
async fn blocking<T, F>(work: F) -> Result<T, BackendError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, BackendError> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| BackendError(format!("file task failed: {e}")))?
}

#[tonic::async_trait]
impl BlobStore for FilesystemStore {
    async fn check(&self) -> Result<(), BackendError> {
        let dir = self.dir.clone();
        blocking(move || {
            if !std::fs::metadata(&dir)?.is_dir() {
                return Err(BackendError(format!(
                    "{} is not a directory",
                    dir.display()
                )));
            }
            // Write and remove a file: this fails on an unmounted mountpoint
            // (root's, not the service user's) and on a read-only drive.
            let probe = dir.join(format!(".probe-{}", random_suffix()?));
            create_private(&probe)?.sync_all()?;
            std::fs::remove_file(&probe)?;
            Ok(())
        })
        .await
    }

    async fn put(&self, id: &BlobId, bytes: Vec<u8>) -> Result<(), BackendError> {
        let dir = self.dir.clone();
        let name = hex(id);
        blocking(move || {
            // Write beside the target and rename, so a reader or a crash never
            // sees half a blob.
            let temp = dir.join(format!(".tmp-{name}-{}", random_suffix()?));
            let written = (|| {
                let mut file = create_private(&temp)?;
                file.write_all(&bytes)?;
                file.sync_all()?;
                std::fs::rename(&temp, dir.join(&name))?;
                File::open(&dir)?.sync_all()
            })();
            if written.is_err() {
                let _ = std::fs::remove_file(&temp);
            }
            Ok(written?)
        })
        .await
    }

    async fn get(&self, id: &BlobId) -> Result<Option<Vec<u8>>, BackendError> {
        let path = self.dir.join(hex(id));
        blocking(move || match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        })
        .await
    }

    async fn delete(&self, id: &BlobId) -> Result<(), BackendError> {
        let path = self.dir.join(hex(id));
        blocking(move || match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        })
        .await
    }

    async fn exists(&self, id: &BlobId) -> Result<bool, BackendError> {
        let path = self.dir.join(hex(id));
        blocking(move || match std::fs::metadata(&path) {
            Ok(meta) => Ok(meta.is_file()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        })
        .await
    }
}
