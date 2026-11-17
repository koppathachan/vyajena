//! key-directory on the device: KeyPackages to upload, the last-resort
//! KeyPackage and its weekly refresh, and when the own leaf is due for
//! rotation (client-core task 5.4; open decision #4, design D6).
//!
//! Every KeyPackage carries this device's certificate as its credential and
//! lists `0xF0A1` in its leaf capabilities (key-directory: KeyPackage upload).
//! The last-resort one is also marked with the `LastResort` extension, so
//! OpenMLS keeps its private key after a join and it can be used again; its
//! leaf capabilities list `LastResort` (design D3, task 1.1 finding).
//!
//! The engine uploads a full set after signup or linking, tops up on a
//! low-KeyPackages notice, and replaces the last-resort one when it is
//! [`protocol::key_directory::LAST_RESORT_MAX_AGE`] old. Every upload carries
//! exactly one last-resort KeyPackage, which replaces the previous one on the
//! Pi.

use std::time::{Duration, SystemTime};

use openmls::prelude::tls_codec::{Deserialize as _, Serialize as _};
use openmls::prelude::{CredentialWithKey, KeyPackage, KeyPackageRef};
use openmls_traits::OpenMlsProvider as _;
use openmls_traits::signatures::Signer;
use openmls_traits::storage::StorageProvider as _;
use protocol::group_membership::{ROTATION_MAX_JITTER, own_leaf_rotation_due};
use protocol::key_directory::{ONE_TIME_KEY_PACKAGES_TARGET, last_resort_refresh_due};

use crate::mls::group::{CIPHERSUITE, capabilities};
use crate::mls::{MlsError, MlsProvider};
use crate::net::backoff::random_below;

/// One upload: one-time KeyPackages plus exactly one last-resort KeyPackage.
#[derive(Debug, Clone)]
pub struct Upload {
    /// TLS-serialized one-time KeyPackages.
    pub one_time: Vec<Vec<u8>>,
    /// The TLS-serialized last-resort KeyPackage.
    pub last_resort: Vec<u8>,
    /// Its hash reference (TLS-serialized), to delete its private key once
    /// it is two refreshes old.
    pub last_resort_ref: Vec<u8>,
    /// The one-time KeyPackages' hash references (TLS-serialized), to delete
    /// their private keys if the Pi does not take them.
    pub one_time_refs: Vec<Vec<u8>>,
}

fn build(
    provider: &MlsProvider,
    signer: &impl Signer,
    credential: &CredentialWithKey,
    last_resort: bool,
) -> Result<KeyPackage, MlsError> {
    let mut builder = KeyPackage::builder().leaf_node_capabilities(capabilities());
    if last_resort {
        builder = builder.mark_as_last_resort();
    }
    Ok(builder
        .build(CIPHERSUITE, provider, signer, credential.clone())
        .map_err(|e| MlsError::new("building a KeyPackage", e))?
        .key_package()
        .clone())
}

fn bytes(key_package: &KeyPackage) -> Result<Vec<u8>, MlsError> {
    key_package
        .tls_serialize_detached()
        .map_err(|e| MlsError::new("serializing a KeyPackage", e))
}

fn hash_ref(provider: &MlsProvider, key_package: &KeyPackage) -> Result<Vec<u8>, MlsError> {
    key_package
        .hash_ref(provider.crypto())
        .map_err(|e| MlsError::new("hashing a KeyPackage", e))?
        .tls_serialize_detached()
        .map_err(|e| MlsError::new("serializing a KeyPackage reference", e))
}

/// `one_time` fresh one-time KeyPackages and a fresh last-resort one. Their
/// private keys go into OpenMLS storage (sealed, design D3).
pub fn new_upload(
    provider: &MlsProvider,
    signer: &impl Signer,
    credential: &CredentialWithKey,
    one_time: usize,
) -> Result<Upload, MlsError> {
    let mut packages = Vec::with_capacity(one_time);
    let mut one_time_refs = Vec::with_capacity(one_time);
    for _ in 0..one_time {
        let key_package = build(provider, signer, credential, false)?;
        one_time_refs.push(hash_ref(provider, &key_package)?);
        packages.push(bytes(&key_package)?);
    }
    let last = build(provider, signer, credential, true)?;
    Ok(Upload {
        one_time: packages,
        last_resort: bytes(&last)?,
        last_resort_ref: hash_ref(provider, &last)?,
        one_time_refs,
    })
}

/// How many one-time KeyPackages to upload when the Pi holds `remaining`:
/// enough to get back to [`ONE_TIME_KEY_PACKAGES_TARGET`].
pub fn top_up(remaining: u32) -> usize {
    ONE_TIME_KEY_PACKAGES_TARGET.saturating_sub(usize::try_from(remaining).unwrap_or(usize::MAX))
}

/// Deletes an old last-resort KeyPackage's private key from OpenMLS storage.
pub fn forget(provider: &MlsProvider, hash_ref: &[u8]) -> Result<(), MlsError> {
    let hash_ref = KeyPackageRef::tls_deserialize_exact(hash_ref)
        .map_err(|e| MlsError::new("reading a KeyPackage reference", e))?;
    provider
        .storage()
        .delete_key_package(&hash_ref)
        .map_err(|e| MlsError::new("deleting an old KeyPackage", e))
}

/// Whether the last-resort KeyPackage uploaded at `uploaded_at` should be
/// replaced at `now` (key-directory: Last-resort KeyPackage refresh). One
/// never uploaded is due.
pub fn last_resort_due(uploaded_at: Option<SystemTime>, now: SystemTime) -> bool {
    match uploaded_at {
        None => true,
        Some(at) => last_resort_refresh_due(now.duration_since(at).unwrap_or_default()),
    }
}

/// Whether the own leaf, last replaced at `updated_at`, is due for rotation
/// at `now` (group-membership: Own leaf rotation).
pub fn rotation_due(updated_at: SystemTime, now: SystemTime) -> bool {
    own_leaf_rotation_due(now.duration_since(updated_at).unwrap_or_default())
}

/// A random delay in `[0, ROTATION_MAX_JITTER]` before a due rotation, so
/// devices do not all commit at once (group-membership: Own leaf rotation).
pub fn rotation_jitter() -> Duration {
    let max = u64::try_from(ROTATION_MAX_JITTER.as_millis()).unwrap_or(u64::MAX);
    Duration::from_millis(random_below(max.saturating_add(1)))
}

/// key-directory: Identity key lookup. The identity keys the Pi has for
/// `users`, from `Keys.GetIdentityKeys` (which consumes no KeyPackages). The
/// Pi fails the whole call with `NOT_FOUND` if one user is unknown or has no
/// non-revoked device; then each user is looked up alone, and those it does
/// not know are left out.
pub(crate) async fn identity_keys(
    session: &crate::net::Session,
    users: &[crate::events::UserId],
) -> Result<std::collections::BTreeMap<crate::events::UserId, Vec<u8>>, crate::net::NetError> {
    use crate::events::UserId;
    use crate::net::NetError;
    let call = |users: Vec<crate::pb::UserId>| async move {
        session
            .conn()
            .keys()
            .get_identity_keys(
                session.request(crate::pb::GetIdentityKeysRequest { user_ids: users })?,
            )
            .await
            .map(|response| response.into_inner().keys)
            .map_err(NetError::from)
    };
    let mut found = std::collections::BTreeMap::new();
    if users.is_empty() {
        return Ok(found);
    }
    let keys = match call(users.iter().map(UserId::to_pb).collect()).await {
        Ok(keys) => keys,
        Err(NetError::Rpc(status)) if status.code() == tonic::Code::NotFound && users.len() > 1 => {
            let mut keys = Vec::new();
            for user in users {
                match call(vec![user.to_pb()]).await {
                    Ok(mut one) => keys.append(&mut one),
                    Err(NetError::Rpc(status)) if status.code() == tonic::Code::NotFound => {
                        tracing::debug!(user = %user, "the Pi knows no identity key");
                    }
                    Err(error) => return Err(error),
                }
            }
            keys
        }
        Err(NetError::Rpc(status)) if status.code() == tonic::Code::NotFound => Vec::new(),
        Err(error) => return Err(error),
    };
    for key in keys {
        if let Some(user) = key.user_id.as_ref().map(UserId::from_pb)
            && users.contains(&user)
        {
            found.insert(user, key.identity_public_key);
        }
    }
    Ok(found)
}
