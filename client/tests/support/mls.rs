//! Minimal OpenMLS members over the client's encrypted storage: enough to put
//! real group state into the store and read it back.

use openmls::prelude::tls_codec::{Deserialize as _, Serialize as _};
use openmls::prelude::{
    BasicCredential, Ciphersuite, CredentialWithKey, GroupId, KeyPackage, LeafNodeParameters,
    MlsGroup, MlsGroupJoinConfig, MlsMessageBodyIn, MlsMessageIn, MlsMessageOut,
    PURE_PLAINTEXT_WIRE_FORMAT_POLICY, ProcessedMessageContent, StagedWelcome,
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::OpenMlsProvider as _;

use client::store::Store;

use super::Check;

/// The only ciphersuite (protocol::key_directory::CIPHERSUITE_ID).
pub const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

/// A new MLS signature key. Tests keep it in memory, as the core keeps it in
/// the key store: never in the database.
pub fn signer() -> SignatureKeyPair {
    SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).check("signature key")
}

/// A basic credential naming `name`.
pub fn credential(name: &str, signer: &SignatureKeyPair) -> CredentialWithKey {
    CredentialWithKey {
        credential: BasicCredential::new(name.as_bytes().to_vec()).into(),
        signature_key: signer.public().into(),
    }
}

/// TLS-serializes an outgoing MLS message.
pub fn wire(message: &MlsMessageOut) -> Vec<u8> {
    message.tls_serialize_detached().check("MLS message bytes")
}

fn join_config() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder()
        .wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
        .use_ratchet_tree_extension(true)
        .max_past_epochs(0)
        .build()
}

/// A KeyPackage of `name`, its private parts stored in `store`.
pub fn key_package(store: &Store, name: &str, signer: &SignatureKeyPair) -> KeyPackage {
    KeyPackage::builder()
        .build(CIPHERSUITE, store.mls(), signer, credential(name, signer))
        .check("KeyPackage")
        .key_package()
        .clone()
}

/// Alice creates a group in her store and adds the holder of `key_package`.
/// Returns her group and the Welcome.
pub fn create_and_add(
    store: &Store,
    signer: &SignatureKeyPair,
    key_package: KeyPackage,
) -> (MlsGroup, Vec<u8>) {
    let provider = store.mls();
    let mut group = MlsGroup::builder()
        .ciphersuite(CIPHERSUITE)
        .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
        .use_ratchet_tree_extension(true)
        .max_past_epochs(0)
        .build(provider, signer, credential("alice", signer))
        .check("create group");
    let (_commit, welcome, _) = group
        .add_members(provider, signer, &[key_package])
        .check("add member");
    group.merge_pending_commit(provider).check("merge add");
    (group, wire(&welcome))
}

/// Joins from a Welcome into `store`.
pub fn join(store: &Store, welcome: &[u8]) -> MlsGroup {
    let message = MlsMessageIn::tls_deserialize_exact(welcome).check("Welcome bytes");
    let MlsMessageBodyIn::Welcome(welcome) = message.extract() else {
        panic!("not a Welcome");
    };
    StagedWelcome::new_from_welcome(store.mls(), &join_config(), welcome, None)
        .check("stage Welcome")
        .into_group(store.mls())
        .check("join")
}

/// Loads a group from `store`.
pub fn load(store: &Store, group_id: &GroupId) -> MlsGroup {
    MlsGroup::load(store.mls().storage(), group_id)
        .check("load group")
        .check("stored group")
}

/// Encrypts `text`.
pub fn send(
    store: &Store,
    group: &mut MlsGroup,
    signer: &SignatureKeyPair,
    text: &[u8],
) -> Vec<u8> {
    wire(
        &group
            .create_message(store.mls(), signer, text)
            .check("encrypt"),
    )
}

/// Processes an incoming message: returns application plaintext, or merges a
/// commit and returns `None`.
pub fn receive(
    store: &Store,
    group: &mut MlsGroup,
    bytes: &[u8],
) -> Result<Option<Vec<u8>>, String> {
    let message = MlsMessageIn::tls_deserialize_exact(bytes)
        .map_err(|e| e.to_string())?
        .try_into_protocol_message()
        .map_err(|e| e.to_string())?;
    let processed = group
        .process_message(store.mls(), message)
        .map_err(|e| e.to_string())?;
    match processed.into_content() {
        ProcessedMessageContent::ApplicationMessage(message) => Ok(Some(message.into_bytes())),
        ProcessedMessageContent::StagedCommitMessage(staged) => {
            group
                .merge_staged_commit(store.mls(), *staged)
                .map_err(|e| e.to_string())?;
            Ok(None)
        }
        _ => Err("unexpected message".into()),
    }
}

/// Commits an update of the own leaf and merges it; returns the commit.
pub fn self_update(store: &Store, group: &mut MlsGroup, signer: &SignatureKeyPair) -> Vec<u8> {
    let bundle = group
        .self_update(store.mls(), signer, LeafNodeParameters::default())
        .check("self update");
    let commit = wire(bundle.commit());
    group
        .merge_pending_commit(store.mls())
        .check("merge self update");
    commit
}
