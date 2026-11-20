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
use protocol::group_membership::{
    ADMIN_EXTENSION_TYPE, DM_EXTENSION_TYPE, GroupKind, NAMED_EXTENSION_TYPE, admin_extension_data,
    dm_extension_data, named_extension_data, parse_dm_extension,
};

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
/// added), the DM extension `0xF0A2` (direct-messages: Old apps never join a
/// DM), the named-group extension `0xF0A3` (multi-group: Named groups reach only
/// v3 apps) and `LastResort` (which the last-resort KeyPackage carries).
pub fn capabilities() -> Capabilities {
    Capabilities::new(
        None,
        Some(&[CIPHERSUITE]),
        Some(&[
            ExtensionType::Unknown(ADMIN_EXTENSION_TYPE),
            ExtensionType::Unknown(DM_EXTENSION_TYPE),
            ExtensionType::Unknown(NAMED_EXTENSION_TYPE),
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

/// The server's external sender entry from `ServerInfo.external_sender`.
pub fn server_external_sender(key: &[u8], credential: &[u8]) -> Result<ExternalSender, MlsError> {
    let credential = Credential::tls_deserialize_exact(credential)
        .map_err(|e| MlsError::new("the server's external sender credential", e))?;
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
/// (group-membership: Group creation): the server as the only external sender,
/// the `0xF0A1` extension naming `admin_user_id`, handshakes as
/// `PublicMessage`.
pub fn create(
    provider: &MlsProvider,
    signer: &impl Signer,
    credential: CredentialWithKey,
    group_id: GroupId,
    server: ExternalSender,
    admin_user_id: &str,
) -> Result<MlsGroup, MlsError> {
    let extensions = Extensions::from_vec(vec![
        Extension::ExternalSenders(vec![server]),
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

/// Creates a DM at epoch 0 with this device as its only member
/// (direct-messages: Opening a DM): the server as the only external sender, the
/// `0xF0A2` extension naming `users`, no admin, handshakes as
/// `PublicMessage`.
pub fn create_dm(
    provider: &MlsProvider,
    signer: &impl Signer,
    credential: CredentialWithKey,
    group_id: GroupId,
    server: ExternalSender,
    users: (&str, &str),
) -> Result<MlsGroup, MlsError> {
    let extensions = Extensions::from_vec(vec![
        Extension::ExternalSenders(vec![server]),
        Extension::Unknown(
            DM_EXTENSION_TYPE,
            UnknownExtension(dm_extension_data(users.0, users.1)),
        ),
    ])
    .map_err(|e| MlsError::new("DM group context extensions", e))?;
    MlsGroup::builder()
        .with_group_id(group_id)
        .ciphersuite(CIPHERSUITE)
        .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
        .use_ratchet_tree_extension(true)
        .max_past_epochs(MAX_PAST_EPOCHS)
        .with_capabilities(capabilities())
        .with_group_context_extensions(extensions)
        .build(provider, signer, credential)
        .map_err(|e| MlsError::new("creating a DM", e))
}

/// Creates a named group at epoch 0 with this device (the admin's) as its only
/// member (multi-group: Named group creation): the server as the only external
/// sender, the `0xF0A1` extension naming `admin_user_id` (so the admin commit
/// authority rule applies) and the `0xF0A3` marker, handshakes as
/// `PublicMessage`.
pub fn create_named_group(
    provider: &MlsProvider,
    signer: &impl Signer,
    credential: CredentialWithKey,
    group_id: GroupId,
    server: ExternalSender,
    admin_user_id: &str,
) -> Result<MlsGroup, MlsError> {
    let extensions = Extensions::from_vec(vec![
        Extension::ExternalSenders(vec![server]),
        Extension::Unknown(
            ADMIN_EXTENSION_TYPE,
            UnknownExtension(admin_extension_data(admin_user_id)),
        ),
        Extension::Unknown(
            NAMED_EXTENSION_TYPE,
            UnknownExtension(named_extension_data()),
        ),
    ])
    .map_err(|e| MlsError::new("named group context extensions", e))?;
    MlsGroup::builder()
        .with_group_id(group_id)
        .ciphersuite(CIPHERSUITE)
        .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
        .use_ratchet_tree_extension(true)
        .max_past_epochs(MAX_PAST_EPOCHS)
        .with_capabilities(capabilities())
        .with_group_context_extensions(extensions)
        .build(provider, signer, credential)
        .map_err(|e| MlsError::new("creating a named group", e))
}

/// The kind a group context says (multi-group: the 3-way classifier): `0xF0A2`
/// is a DM, else `0xF0A3` a named group, else the system lobby.
pub fn kind_of(extensions: &Extensions<openmls::prelude::GroupContext>) -> GroupKind {
    GroupKind::from_extensions(
        extensions.unknown(DM_EXTENSION_TYPE).is_some(),
        extensions.unknown(NAMED_EXTENSION_TYPE).is_some(),
    )
}

/// Whether the group context marks a named group.
pub fn is_named(extensions: &Extensions<openmls::prelude::GroupContext>) -> bool {
    kind_of(extensions) == GroupKind::Named
}

/// The two user ids a DM's `0xF0A2` extension names, sorted; `None` for the
/// main group (or a garbled extension).
pub fn dm_users(
    extensions: &Extensions<openmls::prelude::GroupContext>,
) -> Option<(String, String)> {
    extensions
        .unknown(DM_EXTENSION_TYPE)
        .and_then(|ext| parse_dm_extension(&ext.0))
}

/// Whether the group context marks a DM.
pub fn is_dm(extensions: &Extensions<openmls::prelude::GroupContext>) -> bool {
    extensions.unknown(DM_EXTENSION_TYPE).is_some()
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

/// Position of the server in the group context's external senders.
pub fn pi_sender_index(
    extensions: &Extensions<openmls::prelude::GroupContext>,
    server: &ExternalSender,
) -> Option<u32> {
    extensions
        .external_senders()
        .and_then(|senders| senders.iter().position(|s| s == server))
        .and_then(|index| u32::try_from(index).ok())
}

/// TLS-serializes an outgoing MLS message.
pub fn wire(message: &MlsMessageOut) -> Result<Vec<u8>, MlsError> {
    message
        .tls_serialize_detached()
        .map_err(|e| MlsError::new("serializing an MLS message", e))
}
