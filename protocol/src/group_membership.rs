//! group-membership: commit authority and inactivity.
//!
//! [`check_commit_authority`] is the commit authority rule (Commit authority) on
//! plain types. The server and the device core each read a commit with OpenMLS,
//! describe its proposals as [`Proposal`]s and call it, so both sides apply the
//! same rule (server-core design D3).

use std::time::Duration;

use crate::DAY;

/// A member device that has not connected for this long is inactive
/// (Inactive devices are kept out).
pub const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(7 * DAY);

/// A device commits an update of its own leaf once its leaf is this old (Own
/// leaf rotation; open decision #4). Matches [`INACTIVITY_TIMEOUT`], so a quiet
/// group still moves its keys forward weekly.
pub const OWN_LEAF_MAX_AGE: Duration = Duration::from_secs(7 * DAY);

/// How often a connected device checks whether its leaf is due, besides on
/// every connect (Own leaf rotation).
pub const ROTATION_CHECK_INTERVAL: Duration = Duration::from_secs(DAY);

/// A due rotation waits a random delay in `[0, ROTATION_MAX_JITTER]` so that
/// devices do not all commit at the same moment (Own leaf rotation).
pub const ROTATION_MAX_JITTER: Duration = Duration::from_secs(60 * 60);

/// How many times the server accepts `Keys.RequestReadd` from one device in any
/// [`READD_REQUEST_WINDOW`] (Re-add after an unusable Welcome): enough for a
/// passing fault, not a commit loop.
pub const MAX_READD_REQUESTS_PER_DAY: usize = 3;

/// The sliding window [`MAX_READD_REQUESTS_PER_DAY`] counts in.
pub const READD_REQUEST_WINDOW: Duration = Duration::from_secs(DAY);

/// The longest `RequestReaddRequest.reason` the server accepts, in bytes; the core
/// cuts its error text to this (at a character boundary).
pub const MAX_READD_REASON_BYTES: usize = 512;

/// `reason` cut to at most [`MAX_READD_REASON_BYTES`] bytes at a character
/// boundary.
pub fn readd_reason(reason: &str) -> &str {
    if reason.len() <= MAX_READD_REASON_BYTES {
        return reason;
    }
    let mut end = MAX_READD_REASON_BYTES;
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    reason.get(..end).unwrap_or_default()
}

/// Whether a leaf last updated `leaf_age` ago is due for rotation (Own leaf
/// rotation): at [`OWN_LEAF_MAX_AGE`] or older.
pub fn own_leaf_rotation_due(leaf_age: Duration) -> bool {
    leaf_age >= OWN_LEAF_MAX_AGE
}

/// MLS extension type (RFC 9420 private-use range) of the group context
/// extension that names the admin (Group creation). Its `extension_data` is the
/// admin's user id in its textual form `<username>@<server_id>`, as UTF-8. The
/// Server checks it at `Admin.CreateGroup`. Because it is a group context extension,
/// every device lists this type in its leaf `capabilities.extensions` (in each
/// KeyPackage and leaf update), or OpenMLS refuses to add it.
pub const ADMIN_EXTENSION_TYPE: u16 = 0xF0A1;

/// The `extension_data` of the [`ADMIN_EXTENSION_TYPE`] extension for an admin
/// user id.
pub fn admin_extension_data(admin_user_id: &str) -> Vec<u8> {
    admin_user_id.as_bytes().to_vec()
}

/// MLS extension type of the group context extension that marks a DM and
/// names its two users (direct-messages: Opening a DM, change `dms` design D3).
/// Its `extension_data` is [`dm_extension_data`]. A DM carries no
/// [`ADMIN_EXTENSION_TYPE`] extension. A version 2 core lists this type in the
/// capabilities of every KeyPackage and leaf; a KeyPackage that lists it is
/// DM-capable (direct-messages: Old apps never join a DM).
pub const DM_EXTENSION_TYPE: u16 = 0xF0A2;

/// The `extension_data` of the [`DM_EXTENSION_TYPE`] extension: the two user
/// ids in their textual form, sorted, joined by a newline, as UTF-8. The same
/// for either order of `a` and `b`.
pub fn dm_extension_data(a: &str, b: &str) -> Vec<u8> {
    let (first, second) = if a <= b { (a, b) } else { (b, a) };
    format!("{first}\n{second}").into_bytes()
}

/// The two user ids a [`DM_EXTENSION_TYPE`] extension names, sorted, or `None`
/// if `data` is not exactly two different, non-empty user ids without
/// whitespace, in sorted order, joined by one newline.
pub fn parse_dm_extension(data: &[u8]) -> Option<(String, String)> {
    let text = std::str::from_utf8(data).ok()?;
    let (first, second) = text.split_once('\n')?;
    let valid = |user: &str| !user.is_empty() && !user.chars().any(char::is_whitespace);
    (valid(first) && valid(second) && first < second).then(|| (first.to_owned(), second.to_owned()))
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
    /// The server, as the group's MLS external sender (its inactivity Removes).
    Server,
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
    /// A non-admin added a device certified for another user; in a DM, for a
    /// user outside the pair.
    AddForAnotherUser,
    /// A non-admin removed another user's device, and the server did not propose it.
    RemoveOfAnotherUser,
    /// A non-admin committed an Update of another member's leaf.
    UpdateOfAnotherLeaf,
    /// A non-admin committed a proposal of a kind only the admin may commit, or
    /// a proposal from an external sender other than the server.
    NotAllowed,
}

/// A commit that breaks the commit authority rule; the server answers it with
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
/// - a Remove the server issued as external sender, whatever its target.
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
        (ProposalSender::Server, ProposalKind::Remove { .. }) => None,
        (ProposalSender::Server | ProposalSender::Other, _) => Some(Violation::NotAllowed),
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

/// The DM commit authority rule (group-membership: Commit authority, change
/// `dms` design D4).
///
/// A member device of a DM between `users` may commit only proposals that are
/// each one of:
///
/// - an Update of the committer's own leaf (a commit with no proposals is
///   always allowed);
/// - an Add of a device certified for either of the two users;
/// - a Remove of a device of the committer's own user;
/// - a Remove the server issued as external sender, whatever its target.
///
/// No device has more power in a DM: `committer.is_admin` is ignored. A
/// committer who is not one of the two users breaks the rule with its first
/// proposal, or with [`Violation::NotAllowed`] at index 0 for an empty commit.
pub fn check_dm_commit_authority(
    committer: &Committer<'_>,
    users: (&str, &str),
    proposals: &[Proposal<'_>],
) -> Result<(), CommitAuthorityError> {
    let in_pair = |user: &str| user == users.0 || user == users.1;
    if !in_pair(committer.user) {
        return Err(CommitAuthorityError {
            index: 0,
            violation: Violation::NotAllowed,
        });
    }
    for (index, proposal) in proposals.iter().enumerate() {
        let violation = match (proposal.sender, proposal.kind) {
            (ProposalSender::Member { .. }, ProposalKind::Add { target_user }) => {
                (!in_pair(target_user)).then_some(Violation::AddForAnotherUser)
            }
            _ => member_violation(committer, proposal),
        };
        if let Some(violation) = violation {
            return Err(CommitAuthorityError { index, violation });
        }
    }
    Ok(())
}
