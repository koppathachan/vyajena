//! device-identity: User identity key and device certificates. The user
//! identity key (only on the user's first device), the device auth key and the
//! MLS signature key, all Ed25519, kept only in the key store as 32-byte seeds
//! (client-runtime: Key storage).
//!
//! Key store calls may block (a keychain prompt), so callers on the async
//! runtime run these with `spawn_blocking`.

use ed25519_dalek::SigningKey;
use openmls_traits::signatures::{Signer, SignerError};
use openmls_traits::types::SignatureScheme;
use protocol::device_identity::{DeviceId, device_id};

use crate::keystore::{KeyName, KeyStore, KeyStoreError, create_fresh, get_or_create};

/// Length of a stored Ed25519 seed.
const SEED_LEN: usize = 32;

fn signing_key(secret: &[u8]) -> Result<SigningKey, KeyStoreError> {
    let seed: &[u8; SEED_LEN] = secret
        .try_into()
        .map_err(|_| KeyStoreError::Keychain("a stored key is not 32 bytes".into()))?;
    Ok(SigningKey::from_bytes(seed))
}

/// This device's auth key and MLS signature key.
#[derive(Clone)]
pub struct DeviceKeys {
    auth: SigningKey,
    mls: SigningKey,
}

impl std::fmt::Debug for DeviceKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceKeys")
            .field("device_id", &self.device_id())
            .finish_non_exhaustive()
    }
}

impl DeviceKeys {
    /// The device keys in `store`, if both exist.
    pub fn load(store: &dyn KeyStore) -> Result<Option<Self>, KeyStoreError> {
        let (Some(auth), Some(mls)) = (
            store.get(KeyName::DeviceAuthKey)?,
            store.get(KeyName::MlsSignatureKey)?,
        ) else {
            return Ok(None);
        };
        Ok(Some(Self {
            auth: signing_key(&auth)?,
            mls: signing_key(&mls)?,
        }))
    }

    /// New device keys in `store`, replacing any left by an earlier install
    /// (device-identity: Fresh keys for a new signup).
    pub fn create_fresh(store: &dyn KeyStore) -> Result<Self, KeyStoreError> {
        // A session token saved for the keys being replaced is no use to the
        // new ones (client-runtime: Resume across restart).
        store.delete(KeyName::SessionToken)?;
        let auth = create_fresh(store, KeyName::DeviceAuthKey, SEED_LEN)?;
        let mls = create_fresh(store, KeyName::MlsSignatureKey, SEED_LEN)?;
        Ok(Self {
            auth: signing_key(&auth)?,
            mls: signing_key(&mls)?,
        })
    }

    /// The device keys in `store`, created on first use.
    pub fn load_or_create(store: &dyn KeyStore) -> Result<Self, KeyStoreError> {
        let auth = get_or_create(store, KeyName::DeviceAuthKey, SEED_LEN)?;
        let mls = get_or_create(store, KeyName::MlsSignatureKey, SEED_LEN)?;
        Ok(Self {
            auth: signing_key(&auth)?,
            mls: signing_key(&mls)?,
        })
    }

    /// The device auth key, which answers login challenges.
    pub fn auth(&self) -> &SigningKey {
        &self.auth
    }

    /// The device auth public key.
    pub fn auth_public(&self) -> [u8; 32] {
        self.auth.verifying_key().to_bytes()
    }

    /// The MLS signature public key.
    pub fn mls_public(&self) -> [u8; 32] {
        self.mls.verifying_key().to_bytes()
    }

    /// This device's id: the first 16 bytes of SHA-256 of its auth public key.
    pub fn device_id(&self) -> DeviceId {
        device_id(&self.auth_public())
    }

    /// The MLS signature key as an OpenMLS signer.
    pub fn mls_signer(&self) -> MlsSigner<'_> {
        MlsSigner(&self.mls)
    }
}

/// The MLS signature key as an OpenMLS [`Signer`]. OpenMLS takes the signer as
/// a parameter, so the key never goes into OpenMLS storage (design D3).
pub struct MlsSigner<'a>(&'a SigningKey);

impl Signer for MlsSigner<'_> {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, SignerError> {
        use ed25519_dalek::Signer as _;
        Ok(self.0.sign(payload).to_bytes().to_vec())
    }

    fn signature_scheme(&self) -> SignatureScheme {
        SignatureScheme::ED25519
    }
}

/// The user identity key, if this device holds it (the user's first device).
pub fn identity_key(store: &dyn KeyStore) -> Result<Option<SigningKey>, KeyStoreError> {
    store
        .get(KeyName::IdentityKey)?
        .map(|secret| signing_key(&secret))
        .transpose()
}

/// Creates a new user identity key, replacing any left by an earlier install
/// (device-identity: Fresh keys for a new signup; the iOS Keychain keeps
/// items after the app is deleted).
pub fn create_identity_key(store: &dyn KeyStore) -> Result<SigningKey, KeyStoreError> {
    signing_key(&create_fresh(store, KeyName::IdentityKey, SEED_LEN)?)
}
