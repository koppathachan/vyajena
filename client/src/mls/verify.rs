//! The checks OpenMLS does not do (invariant: OpenMLS skips credential
//! checks), run on every Welcome and every commit before it is merged
//! (group-membership: Device-level membership; Commit authority;
//! key-directory: Clients verify the identity chain; client-core design D5).
//!
//! - Every new or changed leaf carries a device certificate that verifies
//!   against its user's pinned identity key, names a user of this Pi, and
//!   whose MLS key equals the leaf's signature key.
//! - A member's own leaf never changes its certificate or key.
//! - The commit passes `protocol::group_membership::check_commit_authority`.
//!
//! The checks are pure: the engine first collects the users whose identity
//! keys are needed ([`users_in_commit`], [`users_in_welcome`]), pins them
//! (fetching a key seen for the first time), then calls [`review_commit`] or
//! [`check_welcome`] with a lookup of the pinned keys.

use std::collections::BTreeMap;

use openmls::prelude::{
    Credential, LeafNodeIndex, MlsGroup, Proposal, Sender, SenderExtensionIndex, StagedCommit,
};
use protocol::group_membership::{Committer, ProposalKind, ProposalSender, check_commit_authority};

use crate::events::{DeviceId, UserId};
use crate::identity::certificate::{self, CertificateProblem, VerifiedCertificate};
use crate::pb;

/// Why a commit or Welcome was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LeafProblem {
    /// A leaf carries no readable device certificate.
    #[error("a leaf carries no device certificate")]
    NoCertificate,
    /// A leaf's certificate is malformed.
    #[error("a leaf's certificate is malformed: {0}")]
    Malformed(&'static str),
    /// No identity key is pinned for the user a leaf claims.
    #[error("no identity key is pinned for {0}")]
    NotPinned(String),
    /// A leaf's certificate does not verify against its user's pinned key.
    #[error("a certificate for {0} does not verify against the pinned identity key")]
    Unverified(String),
    /// A certificate names a user of another server.
    #[error("a certificate names a user of another server: {0}")]
    ForeignUser(String),
    /// A valid certificate on a different signature key.
    #[error("a certificate's MLS key differs from the leaf's signature key")]
    WrongKey,
    /// A member's leaf changed its certificate or key.
    #[error("a member's leaf changed its certificate or key")]
    LeafChanged,
    /// The commit breaks commit authority.
    #[error("the commit breaks commit authority: {0}")]
    Authority(String),
    /// The group context does not name the admin or does not list the Pi.
    #[error("the group context is not this Pi's group: {0}")]
    Context(&'static str),
    /// Something the checks rely on is missing.
    #[error("unexpected commit structure: {0}")]
    Structure(&'static str),
}

impl From<CertificateProblem> for LeafProblem {
    fn from(problem: CertificateProblem) -> Self {
        match problem {
            CertificateProblem::Malformed(why) => LeafProblem::Malformed(why),
            CertificateProblem::Forged => LeafProblem::Unverified(String::new()),
        }
    }
}

/// A leaf as the checks see it.
#[derive(Debug, Clone)]
pub struct LeafClaim {
    /// The signed certificate in its credential.
    pub certificate: pb::SignedDeviceCertificate,
    /// The certificate decoded without checking (to find the user).
    pub claimed: VerifiedCertificate,
    /// The leaf's signature key.
    pub signature_key: Vec<u8>,
}

impl LeafClaim {
    /// Reads a leaf's credential and key.
    pub fn read(credential: &Credential, signature_key: &[u8]) -> Result<Self, LeafProblem> {
        let certificate =
            certificate::from_credential(credential).ok_or(LeafProblem::NoCertificate)?;
        let claimed = certificate::decode(&certificate)?;
        Ok(Self {
            certificate,
            claimed,
            signature_key: signature_key.to_vec(),
        })
    }

    /// The user the leaf claims.
    pub fn user(&self) -> &UserId {
        &self.claimed.user
    }

    /// Checks the leaf: its certificate verifies against the user's pinned
    /// key, names a user of `server_id`, and certifies the leaf's key.
    pub fn verify(
        &self,
        server_id: &str,
        pinned: &dyn Fn(&UserId) -> Option<Vec<u8>>,
    ) -> Result<VerifiedCertificate, LeafProblem> {
        let user = self.user();
        if user.server_id != server_id {
            return Err(LeafProblem::ForeignUser(user.to_string()));
        }
        let key = pinned(user).ok_or_else(|| LeafProblem::NotPinned(user.to_string()))?;
        let verified = certificate::verify(&self.certificate, &key).map_err(|e| match e {
            CertificateProblem::Forged => LeafProblem::Unverified(user.to_string()),
            other => other.into(),
        })?;
        if verified.mls_signature_public_key != self.signature_key {
            return Err(LeafProblem::WrongKey);
        }
        Ok(verified)
    }

    fn same_leaf(&self, other: &LeafClaim) -> bool {
        self.certificate == other.certificate && self.signature_key == other.signature_key
    }
}

/// The member at `leaf`, read.
pub fn member_claim(group: &MlsGroup, leaf: LeafNodeIndex) -> Result<LeafClaim, LeafProblem> {
    let member = group
        .member_at(leaf)
        .ok_or(LeafProblem::Structure("no member at a referenced leaf"))?;
    LeafClaim::read(&member.credential, member.signature_key.as_slice())
}

/// The device id at each leaf of the group.
pub fn members(group: &MlsGroup) -> Vec<(u32, LeafClaim)> {
    group
        .members()
        .filter_map(|m| {
            LeafClaim::read(&m.credential, m.signature_key.as_slice())
                .ok()
                .map(|claim| (m.index.u32(), claim))
        })
        .collect()
}

/// The leaf of `device_id`, if it is a member.
pub fn leaf_of(group: &MlsGroup, device_id: &DeviceId) -> Option<u32> {
    members(group)
        .into_iter()
        .find(|(_, claim)| claim.claimed.device_id == *device_id)
        .map(|(leaf, _)| leaf)
}

/// The users whose identity keys the review of `staged` needs, with a device
/// id each (to fetch the key with): the users of added leaves.
pub fn users_in_commit(staged: &StagedCommit) -> BTreeMap<UserId, DeviceId> {
    let mut users = BTreeMap::new();
    for add in staged.add_proposals() {
        let leaf = add.add_proposal().key_package().leaf_node();
        if let Ok(claim) = LeafClaim::read(leaf.credential(), leaf.signature_key().as_slice()) {
            users
                .entry(claim.claimed.user.clone())
                .or_insert(claim.claimed.device_id);
        }
    }
    users
}

/// The users whose identity keys a Welcome's check needs: every member's.
pub fn users_in_welcome(
    members: impl Iterator<Item = openmls::prelude::Member>,
) -> BTreeMap<UserId, DeviceId> {
    let mut users = BTreeMap::new();
    for member in members {
        if let Ok(claim) = LeafClaim::read(&member.credential, member.signature_key.as_slice()) {
            users
                .entry(claim.claimed.user.clone())
                .or_insert(claim.claimed.device_id);
        }
    }
    users
}

/// What an accepted commit changes, for events and bookkeeping.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitReview {
    /// Devices added, with their users.
    pub added: Vec<(UserId, DeviceId)>,
    /// Devices removed, with their users.
    pub removed: Vec<(UserId, DeviceId)>,
    /// Whether this device is removed.
    pub self_removed: bool,
}

/// Owned form of a proposal for the rule.
enum Kind {
    Add(String),
    Update,
    Remove(String),
    Other(u16),
}

/// Reviews a commit another member sent, before it is merged: certificates
/// and keys of new and changed leaves, then commit authority.
///
/// `committer` is the sender leaf, `server_id` this Pi's, `pi` the Pi's
/// position in the external senders, `pinned` the pinned identity keys.
pub fn review_commit(
    group: &MlsGroup,
    staged: &StagedCommit,
    committer: LeafNodeIndex,
    server_id: &str,
    pi: Option<u32>,
    pinned: &dyn Fn(&UserId) -> Option<Vec<u8>>,
) -> Result<CommitReview, LeafProblem> {
    let committer_claim = member_claim(group, committer)?;
    let committer_user = committer_claim.user().to_string();
    let admin = super::group::admin_user_id(group.extensions())
        .ok_or(LeafProblem::Context("no admin extension"))?;

    // The committer's new path leaf keeps its certificate and key.
    if let Some(leaf) = staged.update_path_leaf_node() {
        let claim = LeafClaim::read(leaf.credential(), leaf.signature_key().as_slice())?;
        if !claim.same_leaf(&committer_claim) {
            return Err(LeafProblem::LeafChanged);
        }
    }

    let mut review = CommitReview::default();
    let mut proposals: Vec<(ProposalSender, Kind)> = Vec::new();
    for queued in staged.queued_proposals() {
        let sender = match queued.sender() {
            Sender::Member(leaf) => ProposalSender::Member {
                leaf_index: leaf.u32(),
            },
            Sender::External(index)
                if pi.is_some_and(|pi| *index == SenderExtensionIndex::new(pi)) =>
            {
                ProposalSender::Pi
            }
            _ => ProposalSender::Other,
        };
        let kind = match queued.proposal() {
            Proposal::Add(add) => {
                let leaf = add.key_package().leaf_node();
                let claim = LeafClaim::read(leaf.credential(), leaf.signature_key().as_slice())?;
                let verified = claim.verify(server_id, pinned)?;
                review
                    .added
                    .push((verified.user.clone(), verified.device_id));
                Kind::Add(verified.user.to_string())
            }
            Proposal::Remove(remove) => {
                let claim = member_claim(group, remove.removed())?;
                review
                    .removed
                    .push((claim.user().clone(), claim.claimed.device_id));
                Kind::Remove(claim.user().to_string())
            }
            Proposal::SelfRemove => {
                let ProposalSender::Member { leaf_index } = sender else {
                    return Err(LeafProblem::Structure("SelfRemove from a non-member"));
                };
                let claim = member_claim(group, LeafNodeIndex::new(leaf_index))?;
                review
                    .removed
                    .push((claim.user().clone(), claim.claimed.device_id));
                Kind::Remove(claim.user().to_string())
            }
            Proposal::Update(update) => {
                let ProposalSender::Member { leaf_index } = sender else {
                    return Err(LeafProblem::Structure("Update from a non-member"));
                };
                let old = member_claim(group, LeafNodeIndex::new(leaf_index))?;
                let leaf = update.leaf_node();
                let new = LeafClaim::read(leaf.credential(), leaf.signature_key().as_slice())?;
                if !new.same_leaf(&old) {
                    return Err(LeafProblem::LeafChanged);
                }
                Kind::Update
            }
            other => Kind::Other(u16::from(other.proposal_type())),
        };
        proposals.push((sender, kind));
    }

    let rule_proposals: Vec<protocol::group_membership::Proposal<'_>> = proposals
        .iter()
        .map(|(sender, kind)| protocol::group_membership::Proposal {
            sender: *sender,
            kind: match kind {
                Kind::Add(user) => ProposalKind::Add { target_user: user },
                Kind::Update => ProposalKind::Update,
                Kind::Remove(user) => ProposalKind::Remove { target_user: user },
                Kind::Other(proposal_type) => ProposalKind::Other {
                    proposal_type: *proposal_type,
                },
            },
        })
        .collect();
    let rule = Committer {
        user: &committer_user,
        is_admin: committer_user == admin,
        leaf_index: committer.u32(),
    };
    check_commit_authority(&rule, &rule_proposals)
        .map_err(|e| LeafProblem::Authority(e.to_string()))?;
    review.self_removed = staged.self_removed();
    Ok(review)
}

/// Whether `committer` is a device of the admin the group context's `0xF0A1`
/// extension names.
pub fn committed_by_admin(group: &MlsGroup, committer: LeafNodeIndex) -> bool {
    let Some(admin) = super::group::admin_user_id(group.extensions()) else {
        return false;
    };
    member_claim(group, committer).is_ok_and(|claim| claim.user().to_string() == admin)
}

/// key-directory: Clients verify the identity chain. When [`review_commit`]
/// refused a commit because a certificate does not verify against its user's
/// pinned key, the user whose identity key the device may look up again: only
/// for a commit from the admin (a reset user being re-added). The commit is
/// then accepted only if the Pi returns a new key that makes the review pass.
/// From any other member, `None`: the commit stays refused.
pub fn key_lookup_allowed(
    group: &MlsGroup,
    committer: LeafNodeIndex,
    problem: &LeafProblem,
) -> Option<UserId> {
    let LeafProblem::Unverified(user) = problem else {
        return None;
    };
    if !committed_by_admin(group, committer) {
        return None;
    }
    let (username, server_id) = user.split_once('@')?;
    Some(UserId::new(username, server_id))
}

/// key-directory: Clients verify the identity chain. When [`check_welcome`]
/// refused a Welcome because a member's certificate does not verify against
/// its user's pinned key, the user whose identity key the device looks up
/// again, whoever signed the Welcome. The Welcome is then joined only if the
/// Pi returns a new key that makes the check pass.
pub fn welcome_key_lookup(problem: &LeafProblem) -> Option<UserId> {
    let LeafProblem::Unverified(user) = problem else {
        return None;
    };
    let (username, server_id) = user.split_once('@')?;
    Some(UserId::new(username, server_id))
}

/// Checks a Welcome's group before joining it: every member's leaf, the admin
/// extension, and the Pi as external sender.
pub fn check_welcome(
    members: impl Iterator<Item = openmls::prelude::Member>,
    context_extensions: &openmls::prelude::Extensions<openmls::prelude::GroupContext>,
    server_id: &str,
    pi: &openmls::prelude::ExternalSender,
    pinned: &dyn Fn(&UserId) -> Option<Vec<u8>>,
) -> Result<(), LeafProblem> {
    if super::group::admin_user_id(context_extensions).is_none() {
        return Err(LeafProblem::Context("no admin extension"));
    }
    if super::group::pi_sender_index(context_extensions, pi).is_none() {
        return Err(LeafProblem::Context("the Pi is not an external sender"));
    }
    for member in members {
        LeafClaim::read(&member.credential, member.signature_key.as_slice())?
            .verify(server_id, pinned)?;
    }
    Ok(())
}
