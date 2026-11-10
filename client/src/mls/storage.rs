//! OpenMLS group state in SQLite, every value encrypted under the storage key
//! (client-core design D3; client-runtime: Key storage).
//!
//! The provider is a key-value store with one table, `mls_storage(slot, value)`,
//! in the same database (and on the same connection) as the message store, so
//! "decrypt, store the message" is one transaction ([`crate::store::Store::transaction`]):
//!
//! - `slot` is HMAC-SHA256 over the OpenMLS label and key (group id, KeyPackage
//!   ref, public key, ...), so lookups are exact and the row ids reveal nothing.
//! - `value` is the serde_json encoding OpenMLS hands us, sealed with
//!   XChaCha20-Poly1305 (random 24-byte nonce) with the slot as associated data,
//!   so a value cannot be moved to another slot.
//!
//! The method layout follows `openmls_memory_storage` 0.6 (MIT), which is the
//! same key-value shape over a `HashMap`. Unlike it, this provider refuses to
//! store signature key pairs: the MLS signature key lives only in the key store.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hmac::{Hmac, Mac};
use openmls_traits::storage::{CURRENT_VERSION, StorageProvider, traits};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::Sha256;

use crate::store::Db;

/// Length of the storage key kept in the key store.
pub const STORAGE_KEY_LEN: usize = 32;

const NONCE_LEN: usize = 24;
const SLOT_CONTEXT: &[u8] = b"vyajena2-mls-storage-slot-v1";
const SEAL_CONTEXT: &[u8] = b"vyajena2-mls-storage-seal-v1";
/// Slot of the key-check row; HMAC slots are 32 bytes, so it cannot collide.
const KEY_CHECK_SLOT: &[u8] = b"key-check";
const KEY_CHECK_VALUE: &[u8] = b"vyajena2-mls-storage-key-check-v1";

/// Why a storage operation failed.
#[derive(Debug, thiserror::Error)]
pub enum MlsStorageError {
    /// SQLite failed.
    #[error("database: {0}")]
    Database(#[from] rusqlite::Error),
    /// A value did not (de)serialize.
    #[error("serialization: {0}")]
    Serialization(#[from] serde_json::Error),
    /// A stored value did not authenticate under the storage key: wrong key or
    /// a tampered row.
    #[error("stored value does not decrypt under the storage key")]
    Decryption,
    /// Encryption failed.
    #[error("encryption failed")]
    Encryption,
    /// The database was written under another storage key.
    #[error("the database was written under another storage key")]
    WrongKey,
    /// OpenMLS tried to store a signature key pair, which lives in the key
    /// store only.
    #[error("signature keys are never written to the database")]
    SignatureKeyRefused,
}

type Result<T> = std::result::Result<T, MlsStorageError>;

/// The storage key, split into a slot key and a sealing key.
struct Cipher {
    slot_key: [u8; 32],
    aead: XChaCha20Poly1305,
}

impl Cipher {
    fn new(storage_key: &[u8; STORAGE_KEY_LEN]) -> Self {
        let slot_key = derive(storage_key, SLOT_CONTEXT);
        let seal_key = derive(storage_key, SEAL_CONTEXT);
        Self {
            slot_key,
            aead: XChaCha20Poly1305::new((&seal_key).into()),
        }
    }

    fn slot(&self, label: &[u8], key: &[u8]) -> Vec<u8> {
        let mut mac = hmac(&self.slot_key);
        mac.update(&CURRENT_VERSION.to_be_bytes());
        mac.update(&u64::try_from(label.len()).unwrap_or(u64::MAX).to_be_bytes());
        mac.update(label);
        mac.update(key);
        mac.finalize().into_bytes().to_vec()
    }

    fn seal(&self, slot: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce).map_err(|_| MlsStorageError::Encryption)?;
        let sealed = self
            .aead
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad: slot,
                },
            )
            .map_err(|_| MlsStorageError::Encryption)?;
        let mut out = nonce.to_vec();
        out.extend_from_slice(&sealed);
        Ok(out)
    }

    fn open(&self, slot: &[u8], stored: &[u8]) -> Result<Vec<u8>> {
        let (nonce, sealed) = stored
            .split_at_checked(NONCE_LEN)
            .ok_or(MlsStorageError::Decryption)?;
        self.aead
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: sealed,
                    aad: slot,
                },
            )
            .map_err(|_| MlsStorageError::Decryption)
    }
}

fn hmac(key: &[u8]) -> Hmac<Sha256> {
    // HMAC accepts keys of any length, so this never fails.
    match <Hmac<Sha256> as Mac>::new_from_slice(key) {
        Ok(mac) => mac,
        Err(_) => unreachable!("HMAC takes any key length"),
    }
}

fn derive(storage_key: &[u8], context: &[u8]) -> [u8; 32] {
    let mut mac = hmac(storage_key);
    mac.update(context);
    mac.finalize().into_bytes().into()
}

/// OpenMLS storage over the store's SQLite connection.
pub struct EncryptedStorage {
    db: Db,
    cipher: Cipher,
}

impl std::fmt::Debug for EncryptedStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptedStorage").finish_non_exhaustive()
    }
}

impl EncryptedStorage {
    /// A provider writing to the `mls_storage` table of `db` under
    /// `storage_key`. Fails with [`MlsStorageError::WrongKey`] when the
    /// database was written under another storage key.
    pub(crate) fn new(db: Db, storage_key: &[u8; STORAGE_KEY_LEN]) -> Result<Self> {
        let storage = Self {
            db,
            cipher: Cipher::new(storage_key),
        };
        storage.check_key()?;
        Ok(storage)
    }

    /// A fixed row sealed under the key: written on first use, opened on
    /// every later one, so a wrong key fails loudly instead of reading as
    /// "no group".
    fn check_key(&self) -> Result<()> {
        match self.get_raw(KEY_CHECK_SLOT) {
            Ok(Some(value)) if value == KEY_CHECK_VALUE => Ok(()),
            Ok(Some(_)) | Err(MlsStorageError::Decryption) => Err(MlsStorageError::WrongKey),
            Ok(None) => self.put_raw(KEY_CHECK_SLOT, KEY_CHECK_VALUE),
            Err(other) => Err(other),
        }
    }

    fn get_raw(&self, slot: &[u8]) -> Result<Option<Vec<u8>>> {
        let stored: Option<Vec<u8>> = self.db.with(|conn| {
            conn.query_row(
                "SELECT value FROM mls_storage WHERE slot = ?1",
                params![slot],
                |row| row.get(0),
            )
            .optional()
        })?;
        stored
            .map(|bytes| self.cipher.open(slot, &bytes))
            .transpose()
    }

    fn put_raw(&self, slot: &[u8], plaintext: &[u8]) -> Result<()> {
        let sealed = self.cipher.seal(slot, plaintext)?;
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO mls_storage (slot, value) VALUES (?1, ?2)
                 ON CONFLICT (slot) DO UPDATE SET value = excluded.value",
                params![slot, sealed],
            )
        })?;
        Ok(())
    }

    fn slot(&self, label: &[u8], key: &impl Serialize) -> Result<Vec<u8>> {
        Ok(self.cipher.slot(label, &serde_json::to_vec(key)?))
    }

    fn get<V: DeserializeOwned>(&self, label: &[u8], key: &impl Serialize) -> Result<Option<V>> {
        let slot = self.slot(label, key)?;
        match self.get_raw(&slot)? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    fn put(&self, label: &[u8], key: &impl Serialize, value: &impl Serialize) -> Result<()> {
        let slot = self.slot(label, key)?;
        self.put_raw(&slot, &serde_json::to_vec(value)?)
    }

    fn del(&self, label: &[u8], key: &impl Serialize) -> Result<()> {
        let slot = self.slot(label, key)?;
        self.db
            .with(|conn| conn.execute("DELETE FROM mls_storage WHERE slot = ?1", params![slot]))?;
        Ok(())
    }

    /// A list is stored as one value: the JSON encodings of its items.
    fn raw_list(&self, slot: &[u8]) -> Result<Vec<Vec<u8>>> {
        match self.get_raw(slot)? {
            Some(bytes) => Ok(serde_json::from_slice(&bytes)?),
            None => Ok(Vec::new()),
        }
    }

    fn list<V: DeserializeOwned>(&self, label: &[u8], key: &impl Serialize) -> Result<Vec<V>> {
        let slot = self.slot(label, key)?;
        self.raw_list(&slot)?
            .iter()
            .map(|item| serde_json::from_slice(item).map_err(MlsStorageError::from))
            .collect()
    }

    fn push(&self, label: &[u8], key: &impl Serialize, item: &impl Serialize) -> Result<()> {
        let slot = self.slot(label, key)?;
        let mut list = self.raw_list(&slot)?;
        list.push(serde_json::to_vec(item)?);
        self.put_raw(&slot, &serde_json::to_vec(&list)?)
    }

    fn remove_from(&self, label: &[u8], key: &impl Serialize, item: &impl Serialize) -> Result<()> {
        let slot = self.slot(label, key)?;
        let mut list = self.raw_list(&slot)?;
        let item = serde_json::to_vec(item)?;
        if let Some(pos) = list.iter().position(|stored| *stored == item) {
            list.remove(pos);
        }
        self.put_raw(&slot, &serde_json::to_vec(&list)?)
    }
}

const KEY_PACKAGE: &[u8] = b"KeyPackage";
const PSK: &[u8] = b"Psk";
const ENCRYPTION_KEY_PAIR: &[u8] = b"EncryptionKeyPair";
const EPOCH_KEY_PAIRS: &[u8] = b"EpochKeyPairs";
const TREE: &[u8] = b"Tree";
const GROUP_CONTEXT: &[u8] = b"GroupContext";
const INTERIM_TRANSCRIPT_HASH: &[u8] = b"InterimTranscriptHash";
const CONFIRMATION_TAG: &[u8] = b"ConfirmationTag";
const JOIN_CONFIG: &[u8] = b"MlsGroupJoinConfig";
const OWN_LEAF_NODES: &[u8] = b"OwnLeafNodes";
const GROUP_STATE: &[u8] = b"GroupState";
const QUEUED_PROPOSAL: &[u8] = b"QueuedProposal";
const PROPOSAL_QUEUE_REFS: &[u8] = b"ProposalQueueRefs";
const OWN_LEAF_NODE_INDEX: &[u8] = b"OwnLeafNodeIndex";
const EPOCH_SECRETS: &[u8] = b"EpochSecrets";
const RESUMPTION_PSK_STORE: &[u8] = b"ResumptionPsk";
const MESSAGE_SECRETS: &[u8] = b"MessageSecrets";

impl StorageProvider<CURRENT_VERSION> for EncryptedStorage {
    type Error = MlsStorageError;

    fn write_mls_join_config<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MlsGroupJoinConfig: traits::MlsGroupJoinConfig<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        config: &MlsGroupJoinConfig,
    ) -> Result<()> {
        self.put(JOIN_CONFIG, group_id, config)
    }

    fn append_own_leaf_node<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNode: traits::LeafNode<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        leaf_node: &LeafNode,
    ) -> Result<()> {
        self.push(OWN_LEAF_NODES, group_id, leaf_node)
    }

    fn queue_proposal<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
        QueuedProposal: traits::QueuedProposal<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        proposal_ref: &ProposalRef,
        proposal: &QueuedProposal,
    ) -> Result<()> {
        self.put(QUEUED_PROPOSAL, &(group_id, proposal_ref), proposal)?;
        self.push(PROPOSAL_QUEUE_REFS, group_id, proposal_ref)
    }

    fn write_tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        TreeSync: traits::TreeSync<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        tree: &TreeSync,
    ) -> Result<()> {
        self.put(TREE, group_id, tree)
    }

    fn write_interim_transcript_hash<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        InterimTranscriptHash: traits::InterimTranscriptHash<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        interim_transcript_hash: &InterimTranscriptHash,
    ) -> Result<()> {
        self.put(INTERIM_TRANSCRIPT_HASH, group_id, interim_transcript_hash)
    }

    fn write_context<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupContext: traits::GroupContext<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_context: &GroupContext,
    ) -> Result<()> {
        self.put(GROUP_CONTEXT, group_id, group_context)
    }

    fn write_confirmation_tag<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ConfirmationTag: traits::ConfirmationTag<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        confirmation_tag: &ConfirmationTag,
    ) -> Result<()> {
        self.put(CONFIRMATION_TAG, group_id, confirmation_tag)
    }

    fn write_group_state<
        GroupState: traits::GroupState<CURRENT_VERSION>,
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_state: &GroupState,
    ) -> Result<()> {
        self.put(GROUP_STATE, group_id, group_state)
    }

    fn write_message_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MessageSecrets: traits::MessageSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        message_secrets: &MessageSecrets,
    ) -> Result<()> {
        self.put(MESSAGE_SECRETS, group_id, message_secrets)
    }

    fn write_resumption_psk_store<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ResumptionPskStore: traits::ResumptionPskStore<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        resumption_psk_store: &ResumptionPskStore,
    ) -> Result<()> {
        self.put(RESUMPTION_PSK_STORE, group_id, resumption_psk_store)
    }

    fn write_own_leaf_index<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNodeIndex: traits::LeafNodeIndex<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        own_leaf_index: &LeafNodeIndex,
    ) -> Result<()> {
        self.put(OWN_LEAF_NODE_INDEX, group_id, own_leaf_index)
    }

    fn write_group_epoch_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupEpochSecrets: traits::GroupEpochSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_epoch_secrets: &GroupEpochSecrets,
    ) -> Result<()> {
        self.put(EPOCH_SECRETS, group_id, group_epoch_secrets)
    }

    fn write_signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
        SignatureKeyPair: traits::SignatureKeyPair<CURRENT_VERSION>,
    >(
        &self,
        _public_key: &SignaturePublicKey,
        _signature_key_pair: &SignatureKeyPair,
    ) -> Result<()> {
        Err(MlsStorageError::SignatureKeyRefused)
    }

    fn write_encryption_key_pair<
        EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>,
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &EncryptionKey,
        key_pair: &HpkeKeyPair,
    ) -> Result<()> {
        self.put(ENCRYPTION_KEY_PAIR, public_key, key_pair)
    }

    fn write_encryption_epoch_key_pairs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        EpochKey: traits::EpochKey<CURRENT_VERSION>,
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        epoch: &EpochKey,
        leaf_index: u32,
        key_pairs: &[HpkeKeyPair],
    ) -> Result<()> {
        self.put(EPOCH_KEY_PAIRS, &(group_id, epoch, leaf_index), &key_pairs)
    }

    fn write_key_package<
        HashReference: traits::HashReference<CURRENT_VERSION>,
        KeyPackage: traits::KeyPackage<CURRENT_VERSION>,
    >(
        &self,
        hash_ref: &HashReference,
        key_package: &KeyPackage,
    ) -> Result<()> {
        self.put(KEY_PACKAGE, hash_ref, key_package)
    }

    fn write_psk<
        PskId: traits::PskId<CURRENT_VERSION>,
        PskBundle: traits::PskBundle<CURRENT_VERSION>,
    >(
        &self,
        psk_id: &PskId,
        psk: &PskBundle,
    ) -> Result<()> {
        self.put(PSK, psk_id, psk)
    }

    fn mls_group_join_config<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MlsGroupJoinConfig: traits::MlsGroupJoinConfig<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<MlsGroupJoinConfig>> {
        self.get(JOIN_CONFIG, group_id)
    }

    fn own_leaf_nodes<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNode: traits::LeafNode<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<LeafNode>> {
        self.list(OWN_LEAF_NODES, group_id)
    }

    fn queued_proposal_refs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<ProposalRef>> {
        self.list(PROPOSAL_QUEUE_REFS, group_id)
    }

    fn queued_proposals<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
        QueuedProposal: traits::QueuedProposal<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<(ProposalRef, QueuedProposal)>> {
        let refs: Vec<ProposalRef> = self.list(PROPOSAL_QUEUE_REFS, group_id)?;
        let mut out = Vec::with_capacity(refs.len());
        for proposal_ref in refs {
            // A ref without its proposal is skipped rather than failing the
            // whole queue; remove_proposal deletes both together.
            if let Some(proposal) = self.get(QUEUED_PROPOSAL, &(group_id, &proposal_ref))? {
                out.push((proposal_ref, proposal));
            }
        }
        Ok(out)
    }

    fn tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        TreeSync: traits::TreeSync<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<TreeSync>> {
        self.get(TREE, group_id)
    }

    fn group_context<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupContext: traits::GroupContext<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupContext>> {
        self.get(GROUP_CONTEXT, group_id)
    }

    fn interim_transcript_hash<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        InterimTranscriptHash: traits::InterimTranscriptHash<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<InterimTranscriptHash>> {
        self.get(INTERIM_TRANSCRIPT_HASH, group_id)
    }

    fn confirmation_tag<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ConfirmationTag: traits::ConfirmationTag<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ConfirmationTag>> {
        self.get(CONFIRMATION_TAG, group_id)
    }

    fn group_state<
        GroupState: traits::GroupState<CURRENT_VERSION>,
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupState>> {
        self.get(GROUP_STATE, group_id)
    }

    fn message_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MessageSecrets: traits::MessageSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<MessageSecrets>> {
        self.get(MESSAGE_SECRETS, group_id)
    }

    fn resumption_psk_store<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ResumptionPskStore: traits::ResumptionPskStore<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ResumptionPskStore>> {
        self.get(RESUMPTION_PSK_STORE, group_id)
    }

    fn own_leaf_index<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNodeIndex: traits::LeafNodeIndex<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<LeafNodeIndex>> {
        self.get(OWN_LEAF_NODE_INDEX, group_id)
    }

    fn group_epoch_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupEpochSecrets: traits::GroupEpochSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupEpochSecrets>> {
        self.get(EPOCH_SECRETS, group_id)
    }

    fn signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
        SignatureKeyPair: traits::SignatureKeyPair<CURRENT_VERSION>,
    >(
        &self,
        _public_key: &SignaturePublicKey,
    ) -> Result<Option<SignatureKeyPair>> {
        Ok(None)
    }

    fn encryption_key_pair<
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
        EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>,
    >(
        &self,
        public_key: &EncryptionKey,
    ) -> Result<Option<HpkeKeyPair>> {
        self.get(ENCRYPTION_KEY_PAIR, public_key)
    }

    fn encryption_epoch_key_pairs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        EpochKey: traits::EpochKey<CURRENT_VERSION>,
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        epoch: &EpochKey,
        leaf_index: u32,
    ) -> Result<Vec<HpkeKeyPair>> {
        Ok(self
            .get(EPOCH_KEY_PAIRS, &(group_id, epoch, leaf_index))?
            .unwrap_or_default())
    }

    fn key_package<
        KeyPackageRef: traits::HashReference<CURRENT_VERSION>,
        KeyPackage: traits::KeyPackage<CURRENT_VERSION>,
    >(
        &self,
        hash_ref: &KeyPackageRef,
    ) -> Result<Option<KeyPackage>> {
        self.get(KEY_PACKAGE, hash_ref)
    }

    fn psk<PskBundle: traits::PskBundle<CURRENT_VERSION>, PskId: traits::PskId<CURRENT_VERSION>>(
        &self,
        psk_id: &PskId,
    ) -> Result<Option<PskBundle>> {
        self.get(PSK, psk_id)
    }

    fn remove_proposal<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        proposal_ref: &ProposalRef,
    ) -> Result<()> {
        self.remove_from(PROPOSAL_QUEUE_REFS, group_id, proposal_ref)?;
        self.del(QUEUED_PROPOSAL, &(group_id, proposal_ref))
    }

    fn delete_own_leaf_nodes<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<()> {
        self.del(OWN_LEAF_NODES, group_id)
    }

    fn delete_group_config<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<()> {
        self.del(JOIN_CONFIG, group_id)
    }

    fn delete_tree<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<()> {
        self.del(TREE, group_id)
    }

    fn delete_confirmation_tag<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<()> {
        self.del(CONFIRMATION_TAG, group_id)
    }

    fn delete_group_state<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<()> {
        self.del(GROUP_STATE, group_id)
    }

    fn delete_context<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<()> {
        self.del(GROUP_CONTEXT, group_id)
    }

    fn delete_interim_transcript_hash<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<()> {
        self.del(INTERIM_TRANSCRIPT_HASH, group_id)
    }

    fn delete_message_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<()> {
        self.del(MESSAGE_SECRETS, group_id)
    }

    fn delete_all_resumption_psk_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<()> {
        self.del(RESUMPTION_PSK_STORE, group_id)
    }

    fn delete_own_leaf_index<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<()> {
        self.del(OWN_LEAF_NODE_INDEX, group_id)
    }

    fn delete_group_epoch_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<()> {
        self.del(EPOCH_SECRETS, group_id)
    }

    fn clear_proposal_queue<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<()> {
        let refs: Vec<ProposalRef> = self.list(PROPOSAL_QUEUE_REFS, group_id)?;
        for proposal_ref in refs {
            self.del(QUEUED_PROPOSAL, &(group_id, proposal_ref))?;
        }
        self.del(PROPOSAL_QUEUE_REFS, group_id)
    }

    fn delete_signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
    >(
        &self,
        _public_key: &SignaturePublicKey,
    ) -> Result<()> {
        Ok(())
    }

    fn delete_encryption_key_pair<EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>>(
        &self,
        public_key: &EncryptionKey,
    ) -> Result<()> {
        self.del(ENCRYPTION_KEY_PAIR, public_key)
    }

    fn delete_encryption_epoch_key_pairs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        EpochKey: traits::EpochKey<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        epoch: &EpochKey,
        leaf_index: u32,
    ) -> Result<()> {
        self.del(EPOCH_KEY_PAIRS, &(group_id, epoch, leaf_index))
    }

    fn delete_key_package<KeyPackageRef: traits::HashReference<CURRENT_VERSION>>(
        &self,
        hash_ref: &KeyPackageRef,
    ) -> Result<()> {
        self.del(KEY_PACKAGE, hash_ref)
    }

    fn delete_psk<PskKey: traits::PskId<CURRENT_VERSION>>(&self, psk_id: &PskKey) -> Result<()> {
        self.del(PSK, psk_id)
    }
}
