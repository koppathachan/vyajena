//! The OS keychain through the `keyring` crate (design D3, task 1.2).

use zeroize::Zeroizing;

use super::{KeyName, KeyStore, KeyStoreError, KeyStoreKind, Secret};

/// The keychain service name of every vyajena2 entry. The account is
/// `<profile>/<key name>`.
pub const KEYCHAIN_SERVICE: &str = "vyajena2";

/// The probe entry's key name; never written.
const PROBE: &str = "availability-probe";

/// Keys in the OS keychain: macOS Keychain, Linux Secret Service, Windows
/// Credential Manager.
#[derive(Debug, Clone)]
pub struct OsKeychain {
    profile: String,
}

impl OsKeychain {
    /// The keychain for `profile`, if this machine has a usable one: the
    /// platform store initialised and a read of a probe entry answered (with
    /// "no entry"). Anything else is [`KeyStoreError::NoKeychain`].
    pub fn probe(profile: &str) -> Result<Self, KeyStoreError> {
        let unavailable = |reason: String| KeyStoreError::NoKeychain { reason };
        if let Err(error) = keyring::Entry::store_status() {
            return Err(unavailable(error.to_string()));
        }
        let keychain = Self {
            profile: profile.to_owned(),
        };
        let probe = keychain
            .entry(PROBE)
            .map_err(|error| unavailable(error.to_string()))?;
        match probe.get_secret() {
            Ok(_) | Err(keyring::Error::NoEntry) => Ok(keychain),
            Err(error) => Err(unavailable(error.to_string())),
        }
    }

    /// The profile this keychain stores keys for.
    pub fn profile(&self) -> &str {
        &self.profile
    }

    fn entry(&self, name: &str) -> Result<keyring::Entry, KeyStoreError> {
        keyring::Entry::new(KEYCHAIN_SERVICE, &format!("{}/{name}", self.profile))
            .map_err(keychain_error)
    }
}

fn keychain_error(error: keyring::Error) -> KeyStoreError {
    KeyStoreError::Keychain(error.to_string())
}

impl KeyStore for OsKeychain {
    fn kind(&self) -> KeyStoreKind {
        KeyStoreKind::OsKeychain
    }

    fn get(&self, name: KeyName) -> Result<Option<Secret>, KeyStoreError> {
        match self.entry(name.as_str())?.get_secret() {
            Ok(secret) => Ok(Some(Zeroizing::new(secret))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(keychain_error(error)),
        }
    }

    fn set(&self, name: KeyName, secret: &[u8]) -> Result<(), KeyStoreError> {
        self.entry(name.as_str())?
            .set_secret(secret)
            .map_err(keychain_error)
    }

    fn delete(&self, name: KeyName) -> Result<(), KeyStoreError> {
        match self.entry(name.as_str())?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(keychain_error(error)),
        }
    }
}
