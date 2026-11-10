//! Shared test helpers. Each test file uses a different subset.
#![allow(dead_code)]

pub mod mls;

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use client::ManualClock;
use client::keystore::{KeyStore, MemoryKeyStore};
use client::store::Store;

/// Turns a failure into a test panic naming what failed.
pub trait Check<T> {
    /// The value, or a panic with `what` and the error.
    fn check(self, what: &str) -> T;
}

impl<T, E: std::fmt::Debug> Check<T> for Result<T, E> {
    fn check(self, what: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{what} failed: {error:?}"),
        }
    }
}

impl<T> Check<T> for Option<T> {
    fn check(self, what: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{what}: nothing there"),
        }
    }
}

/// 2026-11-10T00:00:00Z, the start of every manual clock.
pub fn start_time() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_794_268_800)
}

/// A manual clock at [`start_time`].
pub fn manual_clock() -> ManualClock {
    ManualClock::new(start_time())
}

/// A fresh temporary directory.
pub fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().check("temp dir")
}

/// Opens the store at `dir/local.sqlite` with `keys`.
pub fn open_store(dir: &Path, keys: &MemoryKeyStore) -> Store {
    Store::open(&dir.join("local.sqlite"), keys).check("open store")
}

/// A memory key store as a trait object.
pub fn shared(keys: &MemoryKeyStore) -> Arc<dyn KeyStore> {
    Arc::new(keys.clone())
}

/// Whether `haystack` contains `needle`.
pub fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

/// The database file plus its WAL, as one byte string (what a copy of the
/// data directory would hold).
pub fn database_bytes(path: &Path) -> Vec<u8> {
    let mut bytes = std::fs::read(path).check("read database");
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    if let Ok(more) = std::fs::read(&wal) {
        bytes.extend_from_slice(&more);
    }
    bytes
}
