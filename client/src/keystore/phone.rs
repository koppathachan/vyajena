//! The phone key stores (flutter-app design D4): the iOS Keychain and the
//! Android Keystore, both through `keyring-core` 1.0 and a native store crate.
//!
//! - iOS: `apple-native-keyring-store`'s data-protection keychain
//!   (`protected`), every item `WhenUnlockedThisDeviceOnly`: readable only
//!   while the phone is unlocked, never synced to iCloud or restored to
//!   another device.
//! - Android: `android-native-keyring-store`, which seals each secret with
//!   AES-GCM under a non-exportable Android Keystore key and keeps only the
//!   ciphertext in an app-private SharedPreferences file. It needs the app's
//!   `Context`, which `MainActivity` hands to `ndk-context` before Flutter
//!   starts (design D4).
//!
//! Both run inside the Rust core, so keys never pass through Dart. The entry
//! naming and error mapping are shared ([`PhoneKeyStore::with_store`]), so
//! host tests drive them with `keyring-core`'s mock store.

use std::collections::HashMap;
use std::sync::Arc;

use keyring_core::{CredentialStore, Entry};
use zeroize::Zeroizing;

use super::{KEYCHAIN_SERVICE, KeyName, KeyStore, KeyStoreError, KeyStoreKind, Secret};

/// The probe entry's key name; never written.
const PROBE: &str = "availability-probe";

/// The iOS Keychain accessibility of every entry (design D4).
pub const IOS_ACCESS_POLICY: &str = "WhenUnlockedThisDeviceOnly";

/// The name of the Android store: its SharedPreferences file is
/// `keyring-vyajena2` and its Keystore key belongs to it alone.
pub const ANDROID_STORE_NAME: &str = "vyajena2";

/// Keys in a phone's native key store, one `keyring-core` entry per key:
/// service [`KEYCHAIN_SERVICE`], user `<profile>/<key name>`.
pub struct PhoneKeyStore {
    store: Arc<CredentialStore>,
    kind: KeyStoreKind,
    profile: String,
    modifiers: HashMap<&'static str, &'static str>,
}

impl std::fmt::Debug for PhoneKeyStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhoneKeyStore")
            .field("kind", &self.kind)
            .field("profile", &self.profile)
            .field("modifiers", &self.modifiers)
            .finish_non_exhaustive()
    }
}

impl PhoneKeyStore {
    /// Keys for `profile` in `store`, each entry built with `modifiers`
    /// (the iOS access policy; none on Android). The native constructors
    /// use this; tests pass `keyring_core::mock::Store`.
    pub fn with_store(
        store: Arc<CredentialStore>,
        kind: KeyStoreKind,
        profile: &str,
        modifiers: &[(&'static str, &'static str)],
    ) -> Self {
        Self {
            store,
            kind,
            profile: profile.to_owned(),
            modifiers: modifiers.iter().copied().collect(),
        }
    }

    /// Checks that the store answers: a read of a probe entry returns a
    /// secret or "no entry". Anything else is [`KeyStoreError::NoKeychain`].
    pub fn probe(self) -> Result<Self, KeyStoreError> {
        let unavailable = |reason: String| KeyStoreError::NoKeychain { reason };
        let probe = self
            .entry(PROBE)
            .map_err(|error| unavailable(error.to_string()))?;
        match probe.get_secret() {
            Ok(_) | Err(keyring_core::Error::NoEntry) => Ok(self),
            Err(error) => Err(unavailable(error.to_string())),
        }
    }

    /// The iOS Keychain for `profile` (data protection keychain,
    /// [`IOS_ACCESS_POLICY`]), if it answers.
    #[cfg(target_os = "ios")]
    pub fn ios_keychain(profile: &str) -> Result<Self, KeyStoreError> {
        let store = apple_native_keyring_store::protected::Store::new().map_err(|error| {
            KeyStoreError::NoKeychain {
                reason: error.to_string(),
            }
        })?;
        Self::with_store(
            store,
            KeyStoreKind::IosKeychain,
            profile,
            &[("access-policy", IOS_ACCESS_POLICY)],
        )
        .probe()
    }

    /// The Android Keystore store for `profile`, if it answers. Without the
    /// app's `Context` in `ndk-context` (`MainActivity` sets it) the native
    /// store panics; that is caught and reported as
    /// [`KeyStoreError::NoKeychain`].
    #[cfg(target_os = "android")]
    pub fn android_keystore(profile: &str) -> Result<Self, KeyStoreError> {
        let profile = profile.to_owned();
        let open = move || -> Result<Self, KeyStoreError> {
            let config = HashMap::from([("name", ANDROID_STORE_NAME)]);
            let store = android_native_keyring_store::Store::new_with_configuration(&config)
                .map_err(|error| KeyStoreError::NoKeychain {
                    reason: error.to_string(),
                })?;
            Self::with_store(store, KeyStoreKind::AndroidKeystore, &profile, &[]).probe()
        };
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(open)).unwrap_or_else(|_| {
            Err(KeyStoreError::NoKeychain {
                reason: "the Android context is not initialised: MainActivity must call \
                         io.crates.keyring.Keyring.initializeNdkContext first"
                    .into(),
            })
        })
    }

    /// The profile this store keeps keys for.
    pub fn profile(&self) -> &str {
        &self.profile
    }

    fn entry(&self, name: &str) -> Result<Entry, keyring_core::Error> {
        let user = format!("{}/{name}", self.profile);
        let modifiers = (!self.modifiers.is_empty()).then_some(&self.modifiers);
        self.store.build(KEYCHAIN_SERVICE, &user, modifiers)
    }
}

fn store_error(error: keyring_core::Error) -> KeyStoreError {
    // Display only: no variant's message carries secret bytes.
    KeyStoreError::Keychain(error.to_string())
}

impl KeyStore for PhoneKeyStore {
    fn kind(&self) -> KeyStoreKind {
        self.kind
    }

    fn get(&self, name: KeyName) -> Result<Option<Secret>, KeyStoreError> {
        match self.entry(name.as_str()).map_err(store_error)?.get_secret() {
            Ok(secret) => Ok(Some(Zeroizing::new(secret))),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(error) => Err(store_error(error)),
        }
    }

    fn set(&self, name: KeyName, secret: &[u8]) -> Result<(), KeyStoreError> {
        self.entry(name.as_str())
            .map_err(store_error)?
            .set_secret(secret)
            .map_err(store_error)
    }

    fn delete(&self, name: KeyName) -> Result<(), KeyStoreError> {
        match self
            .entry(name.as_str())
            .map_err(store_error)?
            .delete_credential()
        {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(error) => Err(store_error(error)),
        }
    }
}
