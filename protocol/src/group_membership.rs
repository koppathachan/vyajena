//! group-membership: commit authority and inactivity.
//!
//! [`check_commit_authority`] is the commit authority rule (Commit authority) on
//! plain types. The Pi and the device core each read a commit with OpenMLS,
//! describe its proposals as [`Proposal`]s and call it, so both sides apply the
//! same rule (pi-server-core design D3).

use std::time::Duration;

use crate::DAY;

/// A member device that has not connected for this long is inactive
/// (Inactive devices are kept out).
pub const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(7 * DAY);

/// MLS extension type (RFC 9420 private-use range) of the group context
/// extension that names the admin (Group creation). Its `extension_data` is the
/// admin's user id in its textual form `<username>@<server_id>`, as UTF-8. The
/// Pi checks it at `Admin.CreateGroup`. Because it is a group context extension,
/// every device lists this type in its leaf `capabilities.extensions` (in each
/// KeyPackage and leaf update), or OpenMLS refuses to add it.
pub const ADMIN_EXTENSION_TYPE: u16 = 0xF0A1;

/// The `extension_data` of the [`ADMIN_EXTENSION_TYPE`] extension for an admin
/// user id.
pub fn admin_extension_data(admin_user_id: &str) -> Vec<u8> {
    admin_user_id.as_bytes().to_vec()
}

/// The device that sent a commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Committer<'a> {
    /// The committing device's user id, in its textual form
    /// `<username>@<server_id>`.
    pub user: &'a str,
    /// Whether that user is the admin.
    pub is_admin: bool,
    /// The committing device's leaf index in the ratchet tree.
    pub leaf_index: u32,
}

/// Who created a proposal that a commit includes, by value or by reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalSender {
    /// A member, by leaf index. A proposal carried by value in the commit has
    /// the committer as its sender.
    Member {
        /// The proposing member's leaf index.
        leaf_index: u32,
    },
    /// The Pi, as the group's MLS external sender (its inactivity Removes).
    Pi,
    /// Any other external sender or new-member proposal.
    Other,
}

/// What a proposal does, reduced to what the rule needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalKind<'a> {
    /// Adds a device. `target_user` is the user id in the device certificate of
    /// the added KeyPackage's leaf.
    Add {
        /// User id the added device is certified for.
        target_user: &'a str,
    },
    /// Updates the proposal sender's own leaf.
    Update,
    /// Removes a member device. `target_user` is the user id in the device
    /// certificate of the removed leaf.
    Remove {
        /// User id of the removed device.
        target_user: &'a str,
    },
    /// Any other proposal type (PreSharedKey, ReInit, ExternalInit,
    /// GroupContextExtensions, custom ...), by its RFC 9420 code point.
    Other {
        /// The MLS proposal type.
        proposal_type: u16,
    },
}

/// One proposal a commit includes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Proposal<'a> {
    /// Who created it.
    pub sender: ProposalSender,
    /// What it does.
    pub kind: ProposalKind<'a>,
}

/// Why a proposal breaks the commit authority rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Violation {
    /// A non-admin added a device certified for another user.
    AddForAnotherUser,
    /// A non-admin removed another user's device, and the Pi did not propose it.
    RemoveOfAnotherUser,
    /// A non-admin committed an Update of another member's leaf.
    UpdateOfAnotherLeaf,
    /// A non-admin committed a proposal of a kind only the admin may commit, or
    /// a proposal from an external sender other than the Pi.
    NotAllowed,
}

/// A commit that breaks the commit authority rule; the Pi answers it with
/// `REJECTION_REASON_POLICY_VIOLATION`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("proposal {index} breaks commit authority: {violation:?}")]
pub struct CommitAuthorityError {
    /// Position of the first offending proposal in the slice given to
    /// [`check_commit_authority`].
    pub index: usize,
    /// What is wrong with it.
    pub violation: Violation,
}

/// The commit authority rule (Commit authority).
///
/// An admin device may commit any proposals. Any other device may commit only
/// proposals that are each one of:
///
/// - an Update of the committer's own leaf (a commit with no proposals, which
///   only updates the committer's leaf through its path, is always allowed);
/// - an Add of a device certified for the committer's own user;
/// - a Remove of a device of the committer's own user;
/// - a Remove the Pi issued as external sender, whatever its target.
///
/// Returns the first proposal that breaks the rule. Proposal order does not
/// matter (OpenMLS does not keep it).
pub fn check_commit_authority(
    committer: &Committer<'_>,
    proposals: &[Proposal<'_>],
) -> Result<(), CommitAuthorityError> {
    if committer.is_admin {
        return Ok(());
    }
    for (index, proposal) in proposals.iter().enumerate() {
        if let Some(violation) = member_violation(committer, proposal) {
            return Err(CommitAuthorityError { index, violation });
        }
    }
    Ok(())
}

fn member_violation(committer: &Committer<'_>, proposal: &Proposal<'_>) -> Option<Violation> {
    match (proposal.sender, proposal.kind) {
        (ProposalSender::Pi, ProposalKind::Remove { .. }) => None,
        (ProposalSender::Pi | ProposalSender::Other, _) => Some(Violation::NotAllowed),
        (ProposalSender::Member { .. }, ProposalKind::Other { .. }) => Some(Violation::NotAllowed),
        (ProposalSender::Member { leaf_index }, ProposalKind::Update) => {
            (leaf_index != committer.leaf_index).then_some(Violation::UpdateOfAnotherLeaf)
        }
        (ProposalSender::Member { .. }, ProposalKind::Add { target_user }) => {
            (target_user != committer.user).then_some(Violation::AddForAnotherUser)
        }
        (ProposalSender::Member { .. }, ProposalKind::Remove { target_user }) => {
            (target_user != committer.user).then_some(Violation::RemoveOfAnotherUser)
        }
    }
}
