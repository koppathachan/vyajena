//! client-runtime: Key storage. Where the device's keys live (client-core
//! design D3).
//!
//! [`KeyStore`] stores and loads the few named secrets the core has
//! ([`KeyName`]). Implementations:
//!
//! - [`OsKeychain`]: the OS keychain through the `keyring` crate (macOS
//!   Keychain, Linux Secret Service, Windows Credential Manager).
//! - [`FileKeyStore`]: one key file with mode 0600, only with an explicit
//!   opt-in ([`KeyFileOptIn`]) on a machine without a keychain.
//! - [`PhoneKeyStore`]: the iOS Keychain or the Android Keystore through
//!   `keyring-core` (flutter-app design D4), chosen on those targets.
//! - [`MemoryKeyStore`]: in memory, for tests.
//!
//! The core never writes these secrets anywhere else: not to SQLite, logs or other
//! files. [`open_key_store`] picks the store and refuses when there is no
//! keychain and no opt-in.
//!
//! Calls may block for a long time: on macOS a rebuilt binary waits on the
//! keychain's permission dialog (design D3, task 1.2). Async callers run them
//! with `spawn_blocking`.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use zeroize::Zeroizing;

mod file;
mod memory;
mod os;
mod phone;

pub use file::{FileKeyStore, KEY_FILE_MODE, KeyFileOptIn};
pub use memory::MemoryKeyStore;
pub use os::{KEYCHAIN_SERVICE, OsKeychain};
pub use phone::{ANDROID_STORE_NAME, IOS_ACCESS_POLICY, PhoneKeyStore};

/// A secret read from a key store; zeroed when dropped.
pub type Secret = Zeroizing<Vec<u8>>;

/// The secrets the core keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KeyName {
    /// The user identity key (Ed25519), which signs device certificates.
    IdentityKey,
    /// This device's auth key (Ed25519), which answers login challenges.
    DeviceAuthKey,
    /// This device's MLS signature key.
    MlsSignatureKey,
    /// The key that seals OpenMLS group state in SQLite (design D3).
    StorageKey,
    /// The latest session token with its expiry and renewal point, kept so a
    /// restart resumes without the full handshake (client-runtime: Resume
    /// across restart). A bearer secret, so it lives here and nowhere else.
    SessionToken,
}

impl KeyName {
    /// Every key name.
    pub const ALL: [KeyName; 5] = [
        KeyName::IdentityKey,
        KeyName::DeviceAuthKey,
        KeyName::MlsSignatureKey,
        KeyName::StorageKey,
        KeyName::SessionToken,
    ];

    /// The stable name used as the keychain account and in the key file.
    pub const fn as_str(self) -> &'static str {
        match self {
            KeyName::IdentityKey => "identity-key",
            KeyName::DeviceAuthKey => "device-auth-key",
            KeyName::MlsSignatureKey => "mls-signature-key",
            KeyName::StorageKey => "storage-key",
            KeyName::SessionToken => "session-token",
        }
    }

    /// The key name for `name`, if it is one.
    pub fn parse(name: &str) -> Option<KeyName> {
        KeyName::ALL.into_iter().find(|key| key.as_str() == name)
    }
}

impl fmt::Display for KeyName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which kind of store holds the keys, for the UI to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyStoreKind {
    /// The OS keychain (desktop).
    OsKeychain,
    /// The iOS Keychain (data protection keychain, this device only).
    IosKeychain,
    /// The Android Keystore (secrets sealed under a Keystore key).
    AndroidKeystore,
    /// The opt-in 0600 key file.
    File,
    /// In memory (tests).
    Memory,
}

/// Why a key store operation failed.
#[derive(Debug, thiserror::Error)]
pub enum KeyStoreError {
    /// No OS keychain is available and the user did not opt in to a key file:
    /// the core refuses to create keys (client-runtime: Key storage).
    #[error(
        "no OS keychain is available ({reason}). To keep keys in a file only you \
         can read (mode 0600) instead, opt in explicitly by choosing a key file"
    )]
    NoKeychain {
        /// What the keychain reported.
        reason: String,
    },
    /// The OS keychain failed.
    #[error("OS keychain error: {0}")]
    Keychain(String),
    /// Reading or writing the key file failed.
    #[error("{path}: {source}")]
    Io {
        /// The key file.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// The key file is readable or writable by others.
    #[error("{path} has mode {mode:o}; a key file must be mode 0600")]
    InsecureFile {
        /// The key file.
        path: PathBuf,
        /// Its permission bits.
        mode: u32,
    },
    /// The key file is not a key file this version understands.
    #[error("{path} is not a valid key file")]
    CorruptFile {
        /// The key file.
        path: PathBuf,
    },
    /// The OS gave no randomness for a new key.
    #[error("no randomness for a new key: {0}")]
    Random(String),
    /// Key files need Unix file modes.
    #[error("key files are supported only on Unix")]
    Unsupported,
}

/// A store of named secrets.
pub trait KeyStore: Send + Sync + fmt::Debug {
    /// Which kind of store this is.
    fn kind(&self) -> KeyStoreKind;

    /// The secret stored under `name`, if any.
    fn get(&self, name: KeyName) -> Result<Option<Secret>, KeyStoreError>;

    /// Stores `secret` under `name`, replacing any previous one.
    fn set(&self, name: KeyName, secret: &[u8]) -> Result<(), KeyStoreError>;

    /// Deletes the secret under `name`; deleting a missing one is not an error.
    fn delete(&self, name: KeyName) -> Result<(), KeyStoreError>;
}

/// Where the user wants keys kept.
#[derive(Debug, Clone)]
pub struct KeyStoreConfig {
    /// Names this device's keys in the OS keychain, so several clients
    /// (profiles) on one machine do not share keys.
    pub profile: String,
    /// The user's explicit opt-in to a key file, used only when there is no
    /// keychain (or when that file already holds the keys).
    pub key_file: Option<KeyFileOptIn>,
}

/// Opens the key store for `config`: the OS keychain when it is available
/// (on iOS the iOS Keychain, on Android the Android Keystore), otherwise the
/// opted-in key file, otherwise [`KeyStoreError::NoKeychain`].
pub fn open_key_store(config: &KeyStoreConfig) -> Result<Arc<dyn KeyStore>, KeyStoreError> {
    #[cfg(target_os = "ios")]
    let keychain = PhoneKeyStore::ios_keychain(&config.profile);
    #[cfg(target_os = "android")]
    let keychain = PhoneKeyStore::android_keystore(&config.profile);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let keychain = OsKeychain::probe(&config.profile);
    choose_key_store(keychain, config.key_file.as_ref())
}

/// The selection rule of [`open_key_store`], with the keychain probe result
/// passed in so tests can stand in for a machine with or without a keychain.
///
/// 1. An opted-in key file that already exists keeps the keys it holds (they
///    were created there because the machine had no keychain then).
/// 2. Otherwise the keychain, when available.
/// 3. Otherwise the opted-in key file, created now.
/// 4. Otherwise refuse: [`KeyStoreError::NoKeychain`].
pub fn choose_key_store<K: KeyStore + 'static>(
    keychain: Result<K, KeyStoreError>,
    key_file: Option<&KeyFileOptIn>,
) -> Result<Arc<dyn KeyStore>, KeyStoreError> {
    if let Some(opt_in) = key_file
        && opt_in.path().exists()
    {
        return Ok(Arc::new(FileKeyStore::open(opt_in)?));
    }
    match (keychain, key_file) {
        (Ok(keychain), _) => Ok(Arc::new(keychain)),
        (Err(_), Some(opt_in)) => Ok(Arc::new(FileKeyStore::open(opt_in)?)),
        (Err(KeyStoreError::NoKeychain { reason }), None) => {
            Err(KeyStoreError::NoKeychain { reason })
        }
        (Err(other), None) => Err(KeyStoreError::NoKeychain {
            reason: other.to_string(),
        }),
    }
}

/// Writes a fresh random secret of `len` bytes under `name`, replacing any
/// secret left there (device-identity: Fresh keys for a new signup).
pub fn create_fresh(
    store: &dyn KeyStore,
    name: KeyName,
    len: usize,
) -> Result<Secret, KeyStoreError> {
    let mut secret = Zeroizing::new(vec![0u8; len]);
    getrandom::fill(&mut secret).map_err(|error| KeyStoreError::Random(error.to_string()))?;
    let replaced = store.get(name)?.is_some();
    store.set(name, &secret)?;
    tracing::info!(key = %name, kind = ?store.kind(), replaced, "created key");
    Ok(secret)
}

/// Reads `name`, creating a fresh random secret of `len` bytes if it is
/// missing.
pub fn get_or_create(
    store: &dyn KeyStore,
    name: KeyName,
    len: usize,
) -> Result<Secret, KeyStoreError> {
    if let Some(secret) = store.get(name)? {
        return Ok(secret);
    }
    let mut secret = Zeroizing::new(vec![0u8; len]);
    getrandom::fill(&mut secret).map_err(|error| KeyStoreError::Random(error.to_string()))?;
    store.set(name, &secret)?;
    tracing::info!(key = %name, kind = ?store.kind(), "created key");
    Ok(secret)
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> KeyStoreError + '_ {
    move |source| KeyStoreError::Io {
        path: path.to_path_buf(),
        source,
    }
}
