//! client-runtime: Image download and cache. Decrypted WebP bytes (and the OGG/Opus
//! of voice messages, change `audio-messages`) kept as files, one per blob id, in a directory of their own beside (not inside) the
//! message database; this device's index of them is in the store
//! ([`Store::media_put`] and friends).
//!
//! The directory is the OS's to protect: the app places it where the OS
//! encrypts files at rest (iOS Data Protection, Android file-based
//! encryption) and leaves it out of backups. The cache is bounded by an LRU
//! cap on total size. An evicted received image is downloaded again within the
//! blob's 7 days; an image this device sent is kept for those 7 days so it can
//! be resent.

use std::fs::OpenOptions;
use std::io::{ErrorKind, Write as _};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use protocol::blob_store::BLOB_TTL;

use super::{BlobId, MediaError};
use crate::store::Store;

/// The default cap on the cache's total size.
pub const DEFAULT_CACHE_BYTES: u64 = 256 * 1024 * 1024;

/// Mode of the media directory and of its files.
const DIR_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;

/// What a cached blob holds; it decides the file's extension, which a media player
/// reads the format from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    /// A WebP picture.
    Image,
    /// An OGG/Opus voice message.
    Audio,
}

impl MediaKind {
    const ALL: [Self; 2] = [Self::Image, Self::Audio];

    fn extension(self) -> &'static str {
        match self {
            Self::Image => "webp",
            Self::Audio => "opus",
        }
    }
}

/// The device media cache.
#[derive(Debug, Clone)]
pub struct MediaCache {
    dir: PathBuf,
    max_bytes: u64,
}

fn hex(id: &BlobId) -> String {
    use std::fmt::Write as _;
    id.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

impl MediaCache {
    /// A cache in `dir` (made on the first write) holding at most `max_bytes`.
    pub fn new(dir: impl Into<PathBuf>, max_bytes: u64) -> Self {
        Self {
            dir: dir.into(),
            max_bytes,
        }
    }

    /// The directory the files are in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, id: &BlobId, kind: MediaKind) -> PathBuf {
        self.dir.join(format!("{}.{}", hex(id), kind.extension()))
    }

    /// The file `id` is cached in, whatever it holds.
    fn file_of(&self, id: &BlobId) -> Option<PathBuf> {
        MediaKind::ALL
            .into_iter()
            .map(|kind| self.path(id, kind))
            .find(|path| path.is_file())
    }

    /// Caches `webp` under `id`. `own` marks an image this device sent, kept for
    /// the blob's lifetime. Older entries are evicted to stay under the cap.
    pub fn put(
        &self,
        store: &Store,
        id: &BlobId,
        webp: &[u8],
        own: bool,
        now: SystemTime,
    ) -> Result<(), MediaError> {
        self.put_as(store, MediaKind::Image, id, webp, own, now)
    }

    /// [`MediaCache::put`] for a blob of `kind`.
    pub fn put_as(
        &self,
        store: &Store,
        kind: MediaKind,
        id: &BlobId,
        webp: &[u8],
        own: bool,
        now: SystemTime,
    ) -> Result<(), MediaError> {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(DIR_MODE)
            .create(&self.dir)?;
        // Written beside the target and renamed, so a crash never leaves half
        // an image under a blob's name.
        let target = self.path(id, kind);
        let temp = self.dir.join(format!(".{}.tmp", hex(id)));
        let written = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(FILE_MODE)
                .open(&temp)?;
            file.write_all(webp)?;
            file.sync_all()?;
            std::fs::rename(&temp, &target)
        })();
        if let Err(error) = written {
            let _ = std::fs::remove_file(&temp);
            return Err(error.into());
        }
        store.media_put(id, webp.len() as u64, own, now)?;
        self.evict(store, id, now)
    }

    /// The cached image, marked as just shown; `None` if there is none (or its
    /// file is gone, which forgets the entry).
    pub fn get(
        &self,
        store: &Store,
        id: &BlobId,
        now: SystemTime,
    ) -> Result<Option<Vec<u8>>, MediaError> {
        if store.media_entry(id)?.is_none() {
            return Ok(None);
        }
        match std::fs::read(self.path(id, MediaKind::Image)) {
            Ok(bytes) => {
                store.media_touch(id, now)?;
                Ok(Some(bytes))
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                // Only forget an entry whose file is gone altogether: a voice
                // message is cached under another extension.
                if self.file_of(id).is_none() {
                    store.media_remove(id)?;
                }
                Ok(None)
            }
            Err(error) => Err(error.into()),
        }
    }

    /// The path of the cached file `id`, marked as just shown; `None` if there is
    /// none (or its file is gone, which forgets the entry). A media player is
    /// handed this path, so the clip is never copied out of the cache.
    pub fn get_path(
        &self,
        store: &Store,
        id: &BlobId,
        now: SystemTime,
    ) -> Result<Option<PathBuf>, MediaError> {
        if store.media_entry(id)?.is_none() {
            return Ok(None);
        }
        match self.file_of(id) {
            Some(path) => {
                store.media_touch(id, now)?;
                Ok(Some(path))
            }
            None => {
                store.media_remove(id)?;
                Ok(None)
            }
        }
    }

    /// Forgets the image: its file and its index entry.
    pub fn remove(&self, store: &Store, id: &BlobId) -> Result<(), MediaError> {
        self.remove_files(id)?;
        store.media_remove(id)?;
        Ok(())
    }

    fn remove_files(&self, id: &BlobId) -> Result<(), MediaError> {
        for kind in MediaKind::ALL {
            match std::fs::remove_file(self.path(id, kind)) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    /// Whether the image is cached.
    pub fn contains(&self, store: &Store, id: &BlobId) -> Result<bool, MediaError> {
        Ok(store.media_entry(id)?.is_some() && self.file_of(id).is_some())
    }

    /// Deletes the oldest-shown entries until the total is within the cap. An
    /// image this device sent stays for the blob's lifetime, and `keep` (the one
    /// just cached) stays whatever the cap.
    fn evict(&self, store: &Store, keep: &BlobId, now: SystemTime) -> Result<(), MediaError> {
        let entries = store.media_entries_oldest_first()?;
        let mut total: u64 = entries.iter().map(|entry| entry.size).sum();
        for entry in entries {
            if total <= self.max_bytes {
                break;
            }
            let age = now.duration_since(entry.created_at).unwrap_or_default();
            if entry.blob_id.as_slice() == keep.as_slice() || (entry.own && age < BLOB_TTL) {
                continue;
            }
            let Ok(id) = BlobId::try_from(entry.blob_id.as_slice()) else {
                store.media_remove(&entry.blob_id)?;
                continue;
            };
            self.remove_files(&id)?;
            store.media_remove(&entry.blob_id)?;
            total = total.saturating_sub(entry.size);
            tracing::debug!(size = entry.size, "evicted a cached image");
        }
        Ok(())
    }
}
