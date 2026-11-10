//! The one MLS group on the device: its configuration and the helpers the
//! engine uses to create, load and join it (group-membership: Group creation;
//! client-core design D5).

use openmls::prelude::tls_codec::{Deserialize as _, Serialize as _};
use openmls::prelude::{
    Capabilities, Ciphersuite, Credential, CredentialWithKey, Extension, ExtensionType, Extensions,
    ExternalSender, GroupId, MlsGroup, MlsGroupJoinConfig, MlsMessageOut,
    PURE_PLAINTEXT_WIRE_FORMAT_POLICY, SignaturePublicKey, UnknownExtension,
};
use openmls_traits::signatures::Signer;
use protocol::group_membership::{ADMIN_EXTENSION_TYPE, admin_extension_data};

use super::MlsProvider;

/// The only ciphersuite (key-directory: KeyPackage upload).
pub const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

/// Epoch secrets kept after a commit: none (invariant: strict epoch rule,
/// OpenMLS `max_past_epochs = 0`).
pub const MAX_PAST_EPOCHS: usize = 0;

/// Length of the group id this core creates.
pub const GROUP_ID_LEN: usize = 16;

/// Why an MLS step failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("MLS: {0}")]
pub struct MlsError(pub String);

impl MlsError {
    /// An error from `what`.
    pub fn new(what: &str, error: impl std::fmt::Debug) -> Self {
        Self(format!("{what}: {error:?}"))
    }
}

/// Leaf capabilities of every leaf and KeyPackage this device makes: the
/// admin group-context extension `0xF0A1` (without which it could not be
/// added) and `LastResort` (which the last-resort KeyPackage carries).
pub fn capabilities() -> Capabilities {
    Capabilities::new(
        None,
        Some(&[CIPHERSUITE]),
        Some(&[
            ExtensionType::Unknown(ADMIN_EXTENSION_TYPE),
            ExtensionType::LastResort,
        ]),
        None,
        None,
    )
}

/// How this device joins the group from a Welcome: handshakes as
/// `PublicMessage`, the tree in the Welcome, no past epochs.
pub fn join_config() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder()
        .wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
        .use_ratchet_tree_extension(true)
        .max_past_epochs(MAX_PAST_EPOCHS)
        .build()
}

/// The Pi's external sender entry from `ServerInfo.external_sender`.
pub fn pi_external_sender(key: &[u8], credential: &[u8]) -> Result<ExternalSender, MlsError> {
    let credential = Credential::tls_deserialize_exact(credential)
        .map_err(|e| MlsError::new("the Pi's external sender credential", e))?;
    Ok(ExternalSender::new(
        SignaturePublicKey::from(key.to_vec()),
        credential,
    ))
}

/// A fresh random group id.
pub fn new_group_id() -> Result<GroupId, MlsError> {
    let mut id = [0u8; GROUP_ID_LEN];
    getrandom::fill(&mut id).map_err(|e| MlsError::new("randomness", e))?;
    Ok(GroupId::from_slice(&id))
}

/// Creates the group at epoch 0 with this device as its only member
/// (group-membership: Group creation): the Pi as the only external sender,
/// the `0xF0A1` extension naming `admin_user_id`, handshakes as
/// `PublicMessage`.
pub fn create(
    provider: &MlsProvider,
    signer: &impl Signer,
    credential: CredentialWithKey,
    group_id: GroupId,
    pi: ExternalSender,
    admin_user_id: &str,
) -> Result<MlsGroup, MlsError> {
    let extensions = Extensions::from_vec(vec![
        Extension::ExternalSenders(vec![pi]),
        Extension::Unknown(
            ADMIN_EXTENSION_TYPE,
            UnknownExtension(admin_extension_data(admin_user_id)),
        ),
    ])
    .map_err(|e| MlsError::new("group context extensions", e))?;
    MlsGroup::builder()
        .with_group_id(group_id)
        .ciphersuite(CIPHERSUITE)
        .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
        .use_ratchet_tree_extension(true)
        .max_past_epochs(MAX_PAST_EPOCHS)
        .with_capabilities(capabilities())
        .with_group_context_extensions(extensions)
        .build(provider, signer, credential)
        .map_err(|e| MlsError::new("creating the group", e))
}

/// The group stored under `group_id`, if any.
pub fn load(provider: &MlsProvider, group_id: &[u8]) -> Result<Option<MlsGroup>, MlsError> {
    use openmls_traits::OpenMlsProvider as _;
    MlsGroup::load(provider.storage(), &GroupId::from_slice(group_id))
        .map_err(|e| MlsError::new("loading the group", e))
}

/// The admin's user id named by the group context's `0xF0A1` extension.
pub fn admin_user_id(extensions: &Extensions<openmls::prelude::GroupContext>) -> Option<String> {
    extensions
        .unknown(ADMIN_EXTENSION_TYPE)
        .and_then(|ext| String::from_utf8(ext.0.clone()).ok())
}

/// Position of the Pi in the group context's external senders.
pub fn pi_sender_index(
    extensions: &Extensions<openmls::prelude::GroupContext>,
    pi: &ExternalSender,
) -> Option<u32> {
    extensions
        .external_senders()
        .and_then(|senders| senders.iter().position(|s| s == pi))
        .and_then(|index| u32::try_from(index).ok())
}

/// TLS-serializes an outgoing MLS message.
pub fn wire(message: &MlsMessageOut) -> Result<Vec<u8>, MlsError> {
    message
        .tls_serialize_detached()
        .map_err(|e| MlsError::new("serializing an MLS message", e))
}
