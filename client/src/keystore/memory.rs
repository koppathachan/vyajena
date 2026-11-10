//! In-memory keys, for tests.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use zeroize::Zeroizing;

use super::{KeyName, KeyStore, KeyStoreError, KeyStoreKind, Secret};

/// Keys in memory. Clones share the same keys, so a test can "restart" a
/// client with the keys it had.
#[derive(Clone, Default)]
pub struct MemoryKeyStore {
    keys: Arc<Mutex<HashMap<KeyName, Secret>>>,
}

impl MemoryKeyStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl std::fmt::Debug for MemoryKeyStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut names: Vec<KeyName> = self.keys.lock().keys().copied().collect();
        names.sort();
        f.debug_struct("MemoryKeyStore")
            .field("keys", &names)
            .finish()
    }
}

impl KeyStore for MemoryKeyStore {
    fn kind(&self) -> KeyStoreKind {
        KeyStoreKind::Memory
    }

    fn get(&self, name: KeyName) -> Result<Option<Secret>, KeyStoreError> {
        Ok(self.keys.lock().get(&name).cloned())
    }

    fn set(&self, name: KeyName, secret: &[u8]) -> Result<(), KeyStoreError> {
        self.keys
            .lock()
            .insert(name, Zeroizing::new(secret.to_vec()));
        Ok(())
    }

    fn delete(&self, name: KeyName) -> Result<(), KeyStoreError> {
        self.keys.lock().remove(&name);
        Ok(())
    }
}
