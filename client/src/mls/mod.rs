//! OpenMLS on the device: the encrypted storage provider (design D3), the
//! group's configuration ([`group`]) and the checks OpenMLS does not do
//! ([`verify`]). The engine ([`crate::engine`]) drives the group itself.

pub mod group;
mod storage;
pub mod verify;

use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;

pub use group::{CIPHERSUITE, MlsError};
pub use storage::{EncryptedStorage, MlsStorageError, STORAGE_KEY_LEN};

/// The OpenMLS provider of one device: RustCrypto for crypto and randomness,
/// [`EncryptedStorage`] for group state. Get it from
/// [`crate::store::Store::mls`].
///
/// The MLS signature key is not in here: OpenMLS takes the signer as a
/// parameter, and the key lives only in the key store.
#[derive(Debug)]
pub struct MlsProvider {
    crypto: RustCrypto,
    storage: EncryptedStorage,
}

impl MlsProvider {
    pub(crate) fn new(storage: EncryptedStorage) -> Self {
        Self {
            crypto: RustCrypto::default(),
            storage,
        }
    }
}

impl OpenMlsProvider for MlsProvider {
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = EncryptedStorage;

    fn storage(&self) -> &Self::StorageProvider {
        &self.storage
    }

    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }
}
