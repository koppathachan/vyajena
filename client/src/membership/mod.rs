//! group-membership on the device: building the commits this device sends
//! (admin adds and removes, own-device adds and removes, automatic re-add,
//! the Pi's inactivity Removes, own-leaf rotation) and checking fetched
//! KeyPackages before adding them (client-core tasks 5.1 to 5.4, design D5).
//!
//! These are synchronous steps on an `MlsGroup`; the engine fetches what they
//! need, runs them inside a store transaction, sends the result and merges or
//! clears it when the Pi answers.

use openmls::prelude::tls_codec::Deserialize as _;
use openmls::prelude::{
    KeyPackage, KeyPackageIn, LeafNodeIndex, MlsGroup, Proposal, ProtocolVersion, Sender,
    SenderExtensionIndex,
};
use openmls_traits::OpenMlsProvider as _;
use openmls_traits::signatures::Signer;

use crate::events::{DeviceId, UserId};
use crate::identity::VerifiedCertificate;
use crate::identity::certificate;
use crate::mls::verify::{LeafClaim, LeafProblem, members};
use crate::mls::{MlsError, MlsProvider, group::wire};
use crate::pb;

/// A commit this device staged (pending in OpenMLS until the Pi answers).
#[derive(Debug, Clone)]
pub struct StagedOwnCommit {
    /// The epoch it was created in.
    pub epoch: u64,
    /// The TLS-serialized commit (`PublicMessage`).
    pub commit: Vec<u8>,
    /// The TLS-serialized Welcome, when it adds devices.
    pub welcome: Option<Vec<u8>>,
}

/// What to put in one commit.
#[derive(Debug, Default)]
pub struct CommitPlan {
    /// KeyPackages to add.
    pub adds: Vec<KeyPackage>,
    /// Leaves to remove by value.
    pub removes: Vec<LeafNodeIndex>,
    /// Give the commit an update path even without proposals that need one
    /// (own-leaf rotation).
    pub force_path: bool,
}

/// Stages a commit with `plan` plus every pending proposal (the Pi's
/// inactivity Removes; OpenMLS commits the proposal store by reference).
pub fn stage(
    group: &mut MlsGroup,
    provider: &MlsProvider,
    signer: &impl Signer,
    plan: CommitPlan,
) -> Result<StagedOwnCommit, MlsError> {
    let epoch = group.epoch().as_u64();
    let bundle = group
        .commit_builder()
        .propose_adds(plan.adds)
        .propose_removals(plan.removes)
        .force_self_update(plan.force_path)
        .consume_proposal_store(true)
        .load_psks(provider.storage())
        .map_err(|e| MlsError::new("loading PSKs", e))?
        .build(provider.rand(), provider.crypto(), signer, |_| true)
        .map_err(|e| MlsError::new("building a commit", e))?
        .stage_commit(provider)
        .map_err(|e| MlsError::new("staging a commit", e))?;
    let welcome = bundle.to_welcome_msg().map(|w| wire(&w)).transpose()?;
    Ok(StagedOwnCommit {
        epoch,
        commit: wire(bundle.commit())?,
        welcome,
    })
}

/// A KeyPackage the Pi handed out, checked before it is added
/// (key-directory: Clients verify the identity chain; group-membership:
/// Device-level membership): its certificate verifies against the user's
/// identity key and names `user`, the KeyPackage parses and validates, and
/// its leaf carries exactly that certificate and its MLS key.
pub fn check_fetched(
    provider: &MlsProvider,
    identity_key: &[u8],
    user: &UserId,
    entry: &pb::DeviceKeyPackage,
) -> Result<(KeyPackage, VerifiedCertificate), LeafProblem> {
    let signed = entry
        .certificate
        .as_ref()
        .ok_or(LeafProblem::NoCertificate)?;
    let verified = certificate::verify(signed, identity_key).map_err(|e| match e {
        certificate::CertificateProblem::Forged => LeafProblem::Unverified(user.to_string()),
        other => other.into(),
    })?;
    if verified.user != *user {
        return Err(LeafProblem::Unverified(user.to_string()));
    }
    let key_package = KeyPackageIn::tls_deserialize_exact(entry.key_package.as_slice())
        .map_err(|_| LeafProblem::Structure("a fetched KeyPackage does not parse"))?
        .validate(provider.crypto(), ProtocolVersion::Mls10)
        .map_err(|_| LeafProblem::Structure("a fetched KeyPackage is invalid"))?;
    let leaf = key_package.leaf_node();
    let claim = LeafClaim::read(leaf.credential(), leaf.signature_key().as_slice())?;
    if claim.certificate != *signed {
        return Err(LeafProblem::LeafChanged);
    }
    if claim.signature_key != verified.mls_signature_public_key {
        return Err(LeafProblem::WrongKey);
    }
    Ok((key_package, verified))
}

/// The leaves of `user`'s devices in the group.
pub fn leaves_of_user(group: &MlsGroup, user: &UserId) -> Vec<(LeafNodeIndex, DeviceId)> {
    members(group)
        .into_iter()
        .filter(|(_, claim)| claim.user() == user)
        .map(|(leaf, claim)| (LeafNodeIndex::new(leaf), claim.claimed.device_id))
        .collect()
}

/// The encryption key of `device`'s current leaf, if it is a member: a
/// re-add replaces it, so a device that sees a different key than when it
/// got the rejoin notice knows someone else re-added it (group-membership:
/// Automatic re-add, "Two devices race to re-add").
pub fn leaf_fingerprint(group: &MlsGroup, device: &DeviceId) -> Option<(LeafNodeIndex, Vec<u8>)> {
    group.members().find_map(|m| {
        let claim = LeafClaim::read(&m.credential, m.signature_key.as_slice()).ok()?;
        (claim.claimed.device_id == *device).then(|| (m.index, m.encryption_key.clone()))
    })
}

/// Leaves the pending proposals remove.
pub fn pending_removals(group: &MlsGroup) -> Vec<LeafNodeIndex> {
    group
        .pending_proposals()
        .filter_map(|queued| match queued.proposal() {
            Proposal::Remove(remove) => Some(remove.removed()),
            _ => None,
        })
        .collect()
}

/// Whether the Pi (external sender `pi`) has pending Remove proposals.
pub fn has_pending_pi_removes(group: &MlsGroup, pi: Option<u32>) -> bool {
    group.pending_proposals().any(|queued| {
        matches!(queued.proposal(), Proposal::Remove(_))
            && matches!(queued.sender(), Sender::External(index)
                if pi.is_some_and(|pi| *index == SenderExtensionIndex::new(pi)))
    })
}

/// The `DeviceRef` of a device.
pub fn device_ref(user: &UserId, device: &DeviceId) -> pb::DeviceRef {
    pb::DeviceRef {
        user_id: Some(user.to_pb()),
        device_id: device.to_vec(),
    }
}
