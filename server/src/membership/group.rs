//! group-membership: the group's public state on the Pi (Membership state on
//! the Pi), built at `Admin.CreateGroup` (Group creation) and advanced by every
//! accepted commit (Pi validates handshakes without group secrets; Commit
//! authority).
//!
//! The Pi tracks the group with an OpenMLS `PublicGroup` (core-wire-protocol
//! design D5). OpenMLS keeps its state in a key-value store; the Pi uses
//! OpenMLS's in-memory store and saves its contents to the `mls_public_state`
//! table, in the same transaction as everything else an accepted commit
//! writes. All of it runs on the store's single writer thread (design D4).
//!
//! The usual flow, inside one `Store::call` transaction:
//!
//! ```text
//! let mut group = GroupState::load(&tx)?.ok_or(..)?;
//! match group.check_commit(&tx, &caller, server_id, &mls_message)? {
//!     Ok(accepted) => { /* sequence, queue, log ... */ group.save(&tx)?; tx.commit()?; }
//!     Err(rejection) => { /* nothing written; drop `group` */ }
//! }
//! ```

use std::collections::HashMap;
use std::sync::RwLock;

use openmls::prelude::tls_codec::Deserialize as _;
use openmls::prelude::tls_codec::Serialize as _;
use openmls::prelude::{
    BasicCredential, Ciphersuite, ContentType, Credential, ExternalProposal, ExternalSender,
    GroupId, LeafNode, LeafNodeIndex, MlsMessageBodyIn, MlsMessageIn, ProcessedMessageContent,
    Proposal, ProposalStore, ProtocolMessage, PublicGroup, PublicProcessMessageError,
    RatchetTreeIn, Sender, SenderExtensionIndex, SignaturePublicKey, StagedCommit, ValidationError,
};
use openmls_memory_storage::MemoryStorage;
use openmls_rust_crypto::{OpenMlsRustCrypto, RustCrypto};
use openmls_traits::signatures::{Signer, SignerError};
use openmls_traits::types::SignatureScheme;
use protocol::device_identity::DeviceId;
use protocol::group_membership::{
    ADMIN_EXTENSION_TYPE, Committer, ProposalKind, ProposalSender, admin_extension_data,
    check_commit_authority,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use tonic::Status;

use crate::auth::Caller;
use crate::pb;
use crate::runtime::ServerKeys;
use crate::runtime::clock::unix_millis;
use crate::store::StoreError;
use crate::store::records::{self, DeviceRow};

use super::certificate::{self, claimed_device_id};

/// The only ciphersuite the group and KeyPackages may use (key-directory:
/// KeyPackage upload).
pub const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

/// The Pi's MLS external sender entry: its Ed25519 key and a basic credential
/// whose identity is the server id. `GetServerInfo` publishes it, and the group
/// context must list it (Group creation).
pub fn external_sender(server_id: &str, keys: &ServerKeys) -> ExternalSender {
    ExternalSender::new(
        SignaturePublicKey::from(keys.external_sender_public_key().to_vec()),
        Credential::from(BasicCredential::new(server_id.as_bytes().to_vec())),
    )
}

/// The TLS-serialized credential of [`external_sender`], as published in
/// `ServerInfo.external_sender.credential`.
pub fn external_sender_credential(server_id: &str) -> Result<Vec<u8>, StoreError> {
    Credential::from(BasicCredential::new(server_id.as_bytes().to_vec()))
        .tls_serialize_detached()
        .map_err(|e| StoreError::Inconsistent(format!("credential does not serialize: {e}")))
}

/// A device in the group, at a leaf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberDevice {
    /// The device's user's username.
    pub username: String,
    /// The device id.
    pub device_id: DeviceId,
    /// Its leaf index in the ratchet tree.
    pub leaf_index: u32,
}

/// Why a handshake envelope was refused; maps to `RejectionReason`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HandshakeRejection {
    /// Does not parse, is not a `PublicMessage` of the expected content type
    /// for this group, or OpenMLS finds it invalid
    /// (`REJECTION_REASON_MALFORMED`).
    #[error("malformed handshake: {0}")]
    Malformed(String),
    /// Not for the group's current epoch (`REJECTION_REASON_STALE_EPOCH`).
    #[error("handshake is for epoch {sent}, the group is at {current}")]
    StaleEpoch {
        /// The epoch in the message.
        sent: u64,
        /// The group's current epoch.
        current: u64,
    },
    /// Bad signature, a sender that is not the authenticated device, a leaf
    /// whose certificate does not check out, or a commit that breaks commit
    /// authority (`REJECTION_REASON_POLICY_VIOLATION`).
    #[error("policy violation: {0}")]
    PolicyViolation(String),
}

impl HandshakeRejection {
    /// The wire reason.
    pub fn reason(&self) -> pb::RejectionReason {
        match self {
            Self::Malformed(_) => pb::RejectionReason::Malformed,
            Self::StaleEpoch { .. } => pb::RejectionReason::StaleEpoch,
            Self::PolicyViolation(_) => pb::RejectionReason::PolicyViolation,
        }
    }

    /// The group's current epoch, for `SendResult.Rejected.current_epoch`.
    pub fn current_epoch(&self) -> Option<u64> {
        match self {
            Self::StaleEpoch { current, .. } => Some(*current),
            _ => None,
        }
    }
}

fn malformed(why: impl Into<String>) -> HandshakeRejection {
    HandshakeRejection::Malformed(why.into())
}

fn policy(why: impl Into<String>) -> HandshakeRejection {
    HandshakeRejection::PolicyViolation(why.into())
}

/// What an accepted commit changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedCommit {
    /// The epoch the commit was sent in.
    pub from_epoch: u64,
    /// The epoch it creates.
    pub epoch: u64,
    /// The committing device.
    pub committer: MemberDevice,
    /// Devices the commit adds, at their new leaves. A `COMMIT` that adds
    /// devices must carry a Welcome for exactly these (message-delivery:
    /// Welcome travels with its commit).
    pub added: Vec<MemberDevice>,
    /// Devices the commit removes, at their old leaves.
    pub removed: Vec<MemberDevice>,
}

/// Why `Admin.CreateGroup` failed.
#[derive(Debug, thiserror::Error)]
pub enum CreateGroupError {
    /// A group exists already (`ALREADY_EXISTS`).
    #[error("the Pi already holds a group")]
    AlreadyExists,
    /// The public state is not acceptable (`INVALID_ARGUMENT`).
    #[error("invalid group state: {0}")]
    Invalid(String),
    /// The only member is not the calling device (`PERMISSION_DENIED`).
    #[error("the group's only member must be the calling device: {0}")]
    NotCaller(String),
    /// The database failed.
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl From<CreateGroupError> for Status {
    fn from(error: CreateGroupError) -> Self {
        match error {
            CreateGroupError::AlreadyExists => Status::already_exists(error.to_string()),
            CreateGroupError::Invalid(_) => Status::invalid_argument(error.to_string()),
            CreateGroupError::NotCaller(_) => Status::permission_denied(error.to_string()),
            CreateGroupError::Store(error) => error.into(),
        }
    }
}

fn invalid(why: impl Into<String>) -> CreateGroupError {
    CreateGroupError::Invalid(why.into())
}

/// The group's public state, loaded from the database.
pub struct GroupState {
    crypto: RustCrypto,
    storage: MemoryStorage,
    group: PublicGroup,
    pi_sender_index: u32,
}

impl std::fmt::Debug for GroupState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GroupState")
            .field("epoch", &self.epoch())
            .field("pi_sender_index", &self.pi_sender_index)
            .finish_non_exhaustive()
    }
}

fn storage_error(
    what: &str,
) -> impl FnOnce(openmls_memory_storage::MemoryStorageError) -> StoreError + '_ {
    move |e| StoreError::Inconsistent(format!("MLS group state ({what}): {e:?}"))
}

/// Parses TLS-serialized bytes, refusing trailing bytes.
fn parse_exact<T: openmls::prelude::tls_codec::Deserialize>(bytes: &[u8]) -> Option<T> {
    T::tls_deserialize_exact(bytes).ok()
}

impl GroupState {
    /// The group, if one was created.
    pub fn load(conn: &Connection) -> Result<Option<Self>, StoreError> {
        let row: Option<(Vec<u8>, i64)> = conn
            .query_row(
                "SELECT group_id, pi_sender_index FROM mls_group WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((group_id, pi_sender_index)) = row else {
            return Ok(None);
        };
        let mut values = HashMap::new();
        let mut stmt = conn.prepare("SELECT key, value FROM mls_public_state")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        for row in rows {
            let (key, value) = row?;
            values.insert(key, value);
        }
        let storage = MemoryStorage {
            values: RwLock::new(values),
        };
        let group = PublicGroup::load(&storage, &GroupId::from_slice(&group_id))
            .map_err(storage_error("load"))?
            .ok_or_else(|| StoreError::Inconsistent("MLS group state is missing".into()))?;
        Ok(Some(Self {
            crypto: RustCrypto::default(),
            storage,
            group,
            pi_sender_index: u32::try_from(pi_sender_index)
                .map_err(|_| StoreError::Inconsistent("bad pi_sender_index".into()))?,
        }))
    }

    /// Whether a group exists.
    pub fn exists(conn: &Connection) -> Result<bool, StoreError> {
        Ok(conn
            .query_row("SELECT 1 FROM mls_group WHERE id = 1", [], |_| Ok(()))
            .optional()?
            .is_some())
    }

    /// `Admin.CreateGroup` (Group creation): builds the group from its epoch-0
    /// GroupInfo (without the ratchet tree extension) and ratchet tree, checks
    /// it and stores it in `tx`.
    ///
    /// Checks: the GroupInfo and tree parse and verify (OpenMLS), `group_id`
    /// matches, epoch 0, our ciphersuite, the Pi's [`external_sender`] is in
    /// `external_senders`, the [`ADMIN_EXTENSION_TYPE`] extension names
    /// `admin_user_id`, and the only member is `caller`'s device with its
    /// registered certificate and MLS key.
    pub fn create(
        tx: &Transaction<'_>,
        caller: &Caller,
        admin_user_id: &str,
        pi: &ExternalSender,
        request: &pb::CreateGroupRequest,
        now: std::time::SystemTime,
    ) -> Result<Self, CreateGroupError> {
        if Self::exists(tx)? {
            return Err(CreateGroupError::AlreadyExists);
        }
        let message: MlsMessageIn =
            parse_exact(&request.group_info).ok_or_else(|| invalid("group_info does not parse"))?;
        let MlsMessageBodyIn::GroupInfo(group_info) = message.extract() else {
            return Err(invalid("group_info is not a GroupInfo"));
        };
        let tree: RatchetTreeIn = parse_exact(&request.ratchet_tree)
            .ok_or_else(|| invalid("ratchet_tree does not parse"))?;
        if group_info.group_id().as_slice() != request.group_id.as_slice() {
            return Err(invalid("group_id does not match the GroupInfo"));
        }

        let crypto = RustCrypto::default();
        let storage = MemoryStorage::default();
        let (group, _) =
            PublicGroup::from_external(&crypto, &storage, tree, group_info, ProposalStore::new())
                .map_err(|e| invalid(format!("public state rejected: {e}")))?;

        let context = group.group_context();
        if context.epoch().as_u64() != 0 {
            return Err(invalid("the group must be created at epoch 0"));
        }
        if group.ciphersuite() != CIPHERSUITE {
            return Err(invalid(
                "the group must use MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519",
            ));
        }
        let pi_sender_index = context
            .extensions()
            .external_senders()
            .and_then(|senders| senders.iter().position(|s| s == pi))
            .ok_or_else(|| invalid("external_senders does not list the Pi"))?;
        let pi_sender_index =
            u32::try_from(pi_sender_index).map_err(|_| invalid("too many external senders"))?;
        let names_admin = context
            .extensions()
            .unknown(ADMIN_EXTENSION_TYPE)
            .is_some_and(|ext| ext.0 == admin_extension_data(admin_user_id));
        if !names_admin {
            return Err(invalid("the group context does not name the admin"));
        }

        let members: Vec<_> = group.members().collect();
        let [member] = members.as_slice() else {
            return Err(invalid("the new group must have exactly one member"));
        };
        let device = records::device(tx, &caller.device_id)?
            .ok_or_else(|| StoreError::Inconsistent("caller's device is missing".into()))?;
        if !leaf_is_device(&member.credential, &member.signature_key, &device) {
            return Err(CreateGroupError::NotCaller(
                "leaf certificate or key differs".into(),
            ));
        }

        tx.execute(
            "INSERT INTO mls_group (id, group_id, epoch, pi_sender_index, created_at)
             VALUES (1, ?1, 0, ?2, ?3)",
            params![request.group_id, pi_sender_index, unix_millis(now)],
        )
        .map_err(StoreError::from)?;
        let state = Self {
            crypto,
            storage,
            group,
            pi_sender_index,
        };
        state.save(tx)?;
        Ok(state)
    }

    /// Writes the state to `tx`: the OpenMLS store, the epoch and the member
    /// devices with their leaves.
    pub fn save(&self, tx: &Transaction<'_>) -> Result<(), StoreError> {
        tx.execute("DELETE FROM mls_public_state", [])?;
        {
            let values = self
                .storage
                .values
                .read()
                .map_err(|_| StoreError::Inconsistent("MLS state lock poisoned".into()))?;
            let mut insert =
                tx.prepare("INSERT INTO mls_public_state (key, value) VALUES (?1, ?2)")?;
            for (key, value) in values.iter() {
                insert.execute(params![key, value])?;
            }
        }
        tx.execute(
            "UPDATE mls_group SET epoch = ?1 WHERE id = 1",
            params![i64::try_from(self.epoch()).unwrap_or(i64::MAX)],
        )?;
        tx.execute("DELETE FROM mls_members", [])?;
        let mut insert =
            tx.prepare("INSERT INTO mls_members (leaf_index, device_id) VALUES (?1, ?2)")?;
        for member in self.group.members() {
            let device_id = certificate::from_credential(&member.credential)
                .as_ref()
                .and_then(claimed_device_id)
                .ok_or_else(|| StoreError::Inconsistent("member leaf without a device".into()))?;
            insert.execute(params![member.index.u32(), device_id.as_slice()])?;
        }
        // group-membership: Automatic add of new users: a member is never
        // announced again.
        tx.execute(
            "UPDATE devices SET was_member = 1
             WHERE was_member = 0 AND device_id IN (SELECT device_id FROM mls_members)",
            [],
        )?;
        Ok(())
    }

    /// The MLS group id.
    pub fn group_id(&self) -> &[u8] {
        self.group.group_id().as_slice()
    }

    /// The current epoch.
    pub fn epoch(&self) -> u64 {
        self.group.group_context().epoch().as_u64()
    }

    /// Position of the Pi in `external_senders`.
    pub fn pi_sender_index(&self) -> u32 {
        self.pi_sender_index
    }

    /// The OpenMLS public group (read only).
    pub fn public_group(&self) -> &PublicGroup {
        &self.group
    }

    /// The member devices and their leaves, as last saved.
    pub fn members(conn: &Connection) -> Result<Vec<MemberDevice>, StoreError> {
        let mut stmt = conn.prepare(
            "SELECT m.leaf_index, m.device_id, d.username FROM mls_members m
             JOIN devices d ON d.device_id = m.device_id ORDER BY m.leaf_index",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, u32>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut members = Vec::new();
        for row in rows {
            let (leaf_index, device_id, username) = row?;
            members.push(MemberDevice {
                username,
                device_id: records::device_id_from(&device_id)
                    .ok_or_else(|| StoreError::Inconsistent("bad member device id".into()))?,
                leaf_index,
            });
        }
        Ok(members)
    }

    /// Checks a `COMMIT` envelope's MLS message from the authenticated device
    /// `caller` (group-membership: Pi validates handshakes without group
    /// secrets; Commit authority) and, if it passes, merges it into this
    /// in-memory state. Persist it with [`GroupState::save`], or drop the state
    /// if anything else makes the envelope fail.
    ///
    /// Checks, in order: it parses as a `PublicMessage` commit for this group
    /// (else `Malformed`); it is for the current epoch (else `StaleEpoch`);
    /// OpenMLS verifies the signature against the sender's leaf and validates
    /// the commit (a bad signature is a `PolicyViolation`, anything else
    /// `Malformed`); the sender leaf is `caller`'s leaf; every added leaf
    /// carries a certificate of a registered, non-revoked device that verifies
    /// against its user's identity key and names the leaf's signature key; the
    /// committer's new path leaf and any Update keep their device's
    /// certificate and key; and `protocol::group_membership`'s commit
    /// authority rule holds. Membership tags cannot be checked without group
    /// secrets (core-wire-protocol design D5).
    pub fn check_commit(
        &mut self,
        conn: &Connection,
        caller: &Caller,
        server_id: &str,
        mls_message: &[u8],
    ) -> Result<Result<AcceptedCommit, HandshakeRejection>, StoreError> {
        let from_epoch = self.epoch();
        let staged = match self.process(conn, caller, mls_message, ContentType::Commit)? {
            Ok((Processed::Commit(staged), _)) => staged,
            Ok((Processed::Proposal, _)) => return Ok(Err(malformed("not a commit"))),
            Err(rejection) => return Ok(Err(rejection)),
        };
        let committer_leaf = match self.leaf_of(conn, &caller.device_id)? {
            Some(leaf) => leaf,
            None => return Ok(Err(policy("the sender is not a group member"))),
        };
        let committer = MemberDevice {
            username: caller.username.clone(),
            device_id: caller.device_id,
            leaf_index: committer_leaf,
        };
        let review = match self.review_commit(conn, caller, server_id, &staged)? {
            Ok(review) => review,
            Err(rejection) => return Ok(Err(rejection)),
        };

        // A device can hold only one leaf: re-adding a member needs the Remove
        // of its old leaf in the same commit (group-membership: Automatic
        // re-add).
        for (_, device_id) in &review.added {
            let still_member = self.leaf_of(conn, device_id)?.is_some()
                && !review.removed.iter().any(|m| m.device_id == *device_id);
            if still_member {
                return Ok(Err(policy(
                    "an added device is already a member and its old leaf is not removed",
                )));
            }
        }

        let proposals: Vec<protocol::group_membership::Proposal<'_>> = review
            .proposals
            .iter()
            .map(|(sender, kind)| protocol::group_membership::Proposal {
                sender: *sender,
                kind: kind.as_kind(),
            })
            .collect();
        let rule = Committer {
            user: &caller.user_id,
            is_admin: caller.is_admin,
            leaf_index: committer_leaf,
        };
        if let Err(error) = check_commit_authority(&rule, &proposals) {
            return Ok(Err(policy(error.to_string())));
        }

        self.group
            .merge_commit(&self.storage, *staged)
            .map_err(|e| StoreError::Inconsistent(format!("merging an accepted commit: {e:?}")))?;

        // Find where the added devices landed.
        let mut added = Vec::new();
        for (username, device_id) in review.added {
            let leaf = self
                .group
                .members()
                .find(|m| {
                    certificate::from_credential(&m.credential)
                        .as_ref()
                        .and_then(claimed_device_id)
                        == Some(device_id)
                })
                .map(|m| m.index.u32())
                .ok_or_else(|| StoreError::Inconsistent("added device has no leaf".into()))?;
            added.push(MemberDevice {
                username,
                device_id,
                leaf_index: leaf,
            });
        }
        Ok(Ok(AcceptedCommit {
            from_epoch,
            epoch: self.epoch(),
            committer,
            added,
            removed: review.removed,
        }))
    }

    /// Checks a member's `PROPOSAL` envelope MLS message from the
    /// authenticated device `caller` and adds it to the Pi's proposal store, so
    /// a later commit can include it by reference (core-wire-protocol design
    /// D5: the Pi keeps its own proposal store). Same parsing, epoch and sender
    /// checks as [`GroupState::check_commit`]; commit authority applies only
    /// when it is committed. Persist with [`GroupState::save`].
    pub fn check_proposal(
        &mut self,
        conn: &Connection,
        caller: &Caller,
        mls_message: &[u8],
    ) -> Result<Result<(), HandshakeRejection>, StoreError> {
        match self.process(conn, caller, mls_message, ContentType::Proposal)? {
            Ok((Processed::Proposal, Some(proposal))) => {
                self.group
                    .add_proposal(&self.storage, *proposal)
                    .map_err(storage_error("add proposal"))?;
                Ok(Ok(()))
            }
            Ok(_) => Ok(Err(malformed("not a proposal"))),
            Err(rejection) => Ok(Err(rejection)),
        }
    }

    /// Issues the Pi's MLS external Remove proposal for the member at `leaf`
    /// in the current epoch, signed with the Pi's external sender key, and
    /// adds it to the Pi's own proposal store so a member's commit can include
    /// it by reference (group-membership: Inactive devices are kept out;
    /// core-wire-protocol design D5). Returns the TLS-serialized `MLSMessage`
    /// (a `PublicMessage`) for the `PROPOSAL` envelope. Persist with
    /// [`GroupState::save`].
    pub fn issue_external_remove(
        &mut self,
        leaf: u32,
        keys: &ServerKeys,
    ) -> Result<Vec<u8>, StoreError> {
        let inconsistent = |what: &str, e: &dyn std::fmt::Debug| {
            StoreError::Inconsistent(format!("issuing an external Remove ({what}): {e:?}"))
        };
        let signer = PiSigner(keys.external_sender_key());
        let message = ExternalProposal::new_remove::<OpenMlsRustCrypto>(
            LeafNodeIndex::new(leaf),
            self.group.group_id().clone(),
            self.group.group_context().epoch(),
            &signer,
            SenderExtensionIndex::new(self.pi_sender_index),
        )
        .map_err(|e| inconsistent("sign", &e))?;
        let bytes = message
            .tls_serialize_detached()
            .map_err(|e| inconsistent("serialize", &e))?;
        // Run it through the Pi's own PublicGroup, as for a member's proposal.
        let protocol_message = MlsMessageIn::tls_deserialize_exact(bytes.as_slice())
            .map_err(|e| inconsistent("parse", &e))?
            .try_into_protocol_message()
            .map_err(|e| inconsistent("protocol message", &e))?;
        let processed = self
            .group
            .process_message(&self.crypto, protocol_message)
            .map_err(|e| inconsistent("process", &e))?;
        match processed.into_content() {
            ProcessedMessageContent::ProposalMessage(proposal) => self
                .group
                .add_proposal(&self.storage, *proposal)
                .map_err(storage_error("add external proposal"))?,
            _ => return Err(inconsistent("content", &"not a proposal")),
        }
        Ok(bytes)
    }

    /// The leaf of `device_id`, if it is a member.
    pub fn leaf_of(
        &self,
        conn: &Connection,
        device_id: &DeviceId,
    ) -> Result<Option<u32>, StoreError> {
        Ok(conn
            .query_row(
                "SELECT leaf_index FROM mls_members WHERE device_id = ?1",
                params![device_id.as_slice()],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// The member device at `leaf`, as last saved.
    fn member_at(&self, conn: &Connection, leaf: u32) -> Result<Option<MemberDevice>, StoreError> {
        let row: Option<(Vec<u8>, String)> = conn
            .query_row(
                "SELECT m.device_id, d.username FROM mls_members m
                 JOIN devices d ON d.device_id = m.device_id WHERE m.leaf_index = ?1",
                params![leaf],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        row.map(|(device_id, username)| {
            Ok(MemberDevice {
                username,
                device_id: records::device_id_from(&device_id)
                    .ok_or_else(|| StoreError::Inconsistent("bad member device id".into()))?,
                leaf_index: leaf,
            })
        })
        .transpose()
    }

    /// Parses and processes one handshake message from `caller`.
    #[allow(clippy::type_complexity)]
    fn process(
        &self,
        conn: &Connection,
        caller: &Caller,
        mls_message: &[u8],
        expected: ContentType,
    ) -> Result<
        Result<(Processed, Option<Box<openmls::prelude::QueuedProposal>>), HandshakeRejection>,
        StoreError,
    > {
        let Some(message) = parse_exact::<MlsMessageIn>(mls_message) else {
            return Ok(Err(malformed("the MLS message does not parse")));
        };
        let Ok(message) = message.try_into_protocol_message() else {
            return Ok(Err(malformed("not an MLS PublicMessage")));
        };
        if !matches!(message, ProtocolMessage::PublicMessage(_)) {
            return Ok(Err(malformed("handshakes must be PublicMessages")));
        }
        if message.group_id() != self.group.group_id() {
            return Ok(Err(malformed("the message is for another group")));
        }
        if message.content_type() != expected {
            return Ok(Err(malformed(
                "the content type does not match the envelope",
            )));
        }
        let sent = message.epoch().as_u64();
        if sent != self.epoch() {
            return Ok(Err(HandshakeRejection::StaleEpoch {
                sent,
                current: self.epoch(),
            }));
        }
        let Some(own_leaf) = self.leaf_of(conn, &caller.device_id)? else {
            return Ok(Err(policy("the sender is not a group member")));
        };
        let processed = match self.group.process_message(&self.crypto, message) {
            Ok(processed) => processed,
            Err(PublicProcessMessageError::ValidationError(ValidationError::InvalidSignature)) => {
                return Ok(Err(policy(
                    "the signature does not verify against the sender's leaf",
                )));
            }
            Err(error) => return Ok(Err(malformed(format!("rejected by MLS: {error}")))),
        };
        match processed.sender() {
            Sender::Member(leaf) if leaf.u32() == own_leaf => {}
            Sender::Member(_) => {
                return Ok(Err(policy(
                    "the sender leaf is not the authenticated device's leaf",
                )));
            }
            _ => return Ok(Err(policy("the sender is not a member"))),
        }
        Ok(Ok(match processed.into_content() {
            ProcessedMessageContent::StagedCommitMessage(staged) => {
                (Processed::Commit(staged), None)
            }
            ProcessedMessageContent::ProposalMessage(proposal) => {
                (Processed::Proposal, Some(proposal))
            }
            _ => return Ok(Err(malformed("unexpected MLS content"))),
        }))
    }

    /// Reads every proposal of a staged commit against the current tree and
    /// checks the certificates in new or changed leaves.
    fn review_commit(
        &self,
        conn: &Connection,
        caller: &Caller,
        server_id: &str,
        staged: &StagedCommit,
    ) -> Result<Result<Review, HandshakeRejection>, StoreError> {
        let committer_device = records::device(conn, &caller.device_id)?
            .ok_or_else(|| StoreError::Inconsistent("caller's device is missing".into()))?;
        if let Some(leaf) = staged.update_path_leaf_node()
            && !leaf_is_device(
                leaf.credential(),
                leaf.signature_key().as_slice(),
                &committer_device,
            )
        {
            return Ok(Err(policy(
                "the committer's new leaf changes its certificate or key",
            )));
        }

        let mut review = Review::default();
        for queued in staged.queued_proposals() {
            let sender = match queued.sender() {
                Sender::Member(leaf) => ProposalSender::Member {
                    leaf_index: leaf.u32(),
                },
                Sender::External(index)
                    if *index == SenderExtensionIndex::new(self.pi_sender_index) =>
                {
                    ProposalSender::Pi
                }
                _ => ProposalSender::Other,
            };
            let kind = match queued.proposal() {
                Proposal::Add(add) => {
                    let leaf = add.key_package().leaf_node();
                    match self.check_added_leaf(conn, server_id, leaf)? {
                        Ok(device) => {
                            if review.added.iter().any(|(_, id)| *id == device.device_id) {
                                return Ok(Err(policy("the same device is added twice")));
                            }
                            review
                                .added
                                .push((device.username.clone(), device.device_id));
                            Kind::Add(format!("{}@{server_id}", device.username))
                        }
                        Err(rejection) => return Ok(Err(rejection)),
                    }
                }
                Proposal::Remove(remove) => {
                    match self.removed_member(conn, server_id, remove.removed())? {
                        Ok((member, user_id)) => {
                            review.removed.push(member);
                            Kind::Remove(user_id)
                        }
                        Err(rejection) => return Ok(Err(rejection)),
                    }
                }
                Proposal::SelfRemove => {
                    let ProposalSender::Member { leaf_index } = sender else {
                        return Ok(Err(malformed("SelfRemove from a non-member")));
                    };
                    match self.removed_member(conn, server_id, LeafNodeIndex::new(leaf_index))? {
                        Ok((member, user_id)) => {
                            review.removed.push(member);
                            Kind::Remove(user_id)
                        }
                        Err(rejection) => return Ok(Err(rejection)),
                    }
                }
                Proposal::Update(update) => {
                    let ProposalSender::Member { leaf_index } = sender else {
                        return Ok(Err(malformed("Update from a non-member")));
                    };
                    let Some(member) = self.member_at(conn, leaf_index)? else {
                        return Ok(Err(malformed("Update from an unknown leaf")));
                    };
                    let device = records::device(conn, &member.device_id)?.ok_or_else(|| {
                        StoreError::Inconsistent("member device is missing".into())
                    })?;
                    let leaf = update.leaf_node();
                    if !leaf_is_device(leaf.credential(), leaf.signature_key().as_slice(), &device)
                    {
                        return Ok(Err(policy("an Update changes a leaf's certificate or key")));
                    }
                    Kind::Update
                }
                other => Kind::Other(u16::from(other.proposal_type())),
            };
            review.proposals.push((sender, kind));
        }
        Ok(Ok(review))
    }

    /// The registered device an Add's leaf is for, if its certificate checks
    /// out (group-membership: Device-level membership; the Pi checks it too, so
    /// it never sequences an Add honest members would reject for it).
    fn check_added_leaf(
        &self,
        conn: &Connection,
        server_id: &str,
        leaf: &LeafNode,
    ) -> Result<Result<DeviceRow, HandshakeRejection>, StoreError> {
        let Some(signed) = certificate::from_credential(leaf.credential()) else {
            return Ok(Err(policy("an added leaf carries no device certificate")));
        };
        let Some(device_id) = claimed_device_id(&signed) else {
            return Ok(Err(policy("an added leaf's certificate does not decode")));
        };
        let Some(device) = records::device(conn, &device_id)? else {
            return Ok(Err(policy("an added device is not registered")));
        };
        if device.revoked {
            return Ok(Err(policy("an added device is revoked")));
        }
        let user = records::user(conn, &device.username)?
            .ok_or_else(|| StoreError::Inconsistent("device without a user".into()))?;
        match certificate::verify(&signed, &user.identity_public_key) {
            Ok(cert)
                if cert.username == device.username
                    && cert.server_id == server_id
                    && cert.mls_signature_public_key == leaf.signature_key().as_slice() => {}
            _ => {
                return Ok(Err(policy(
                    "an added leaf's certificate does not check out",
                )));
            }
        }
        Ok(Ok(device))
    }

    /// The member at a removed leaf and its user id.
    fn removed_member(
        &self,
        conn: &Connection,
        server_id: &str,
        leaf: LeafNodeIndex,
    ) -> Result<Result<(MemberDevice, String), HandshakeRejection>, StoreError> {
        match self.member_at(conn, leaf.u32())? {
            Some(member) => {
                let user_id = format!("{}@{server_id}", member.username);
                Ok(Ok((member, user_id)))
            }
            None => Ok(Err(malformed("a Remove names an empty leaf"))),
        }
    }
}

/// What processing a handshake produced.
enum Processed {
    Commit(Box<StagedCommit>),
    Proposal,
}

/// An owned description of one proposal, for the commit authority rule.
enum Kind {
    Add(String),
    Update,
    Remove(String),
    Other(u16),
}

impl Kind {
    fn as_kind(&self) -> ProposalKind<'_> {
        match self {
            Kind::Add(user) => ProposalKind::Add { target_user: user },
            Kind::Update => ProposalKind::Update,
            Kind::Remove(user) => ProposalKind::Remove { target_user: user },
            Kind::Other(proposal_type) => ProposalKind::Other {
                proposal_type: *proposal_type,
            },
        }
    }
}

#[derive(Default)]
struct Review {
    proposals: Vec<(ProposalSender, Kind)>,
    added: Vec<(String, DeviceId)>,
    removed: Vec<MemberDevice>,
}

/// Whether a leaf carries exactly `device`'s registered certificate and its
/// MLS signature key.
fn leaf_is_device(credential: &Credential, signature_key: &[u8], device: &DeviceRow) -> bool {
    certificate::from_credential(credential).is_some_and(|signed| {
        signed.certificate == device.certificate && signed.signature == device.certificate_signature
    }) && signature_key == device.mls_signature_public_key.as_slice()
        && !device.revoked
}

/// The Pi's external sender key as an OpenMLS signer.
struct PiSigner<'a>(&'a ed25519_dalek::SigningKey);

impl Signer for PiSigner<'_> {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, SignerError> {
        use ed25519_dalek::Signer as _;
        Ok(self.0.sign(payload).to_bytes().to_vec())
    }

    fn signature_scheme(&self) -> SignatureScheme {
        SignatureScheme::ED25519
    }
}
