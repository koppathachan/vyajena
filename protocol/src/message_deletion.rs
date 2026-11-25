//! message-deletion: who may delete a message (Who may delete).
//!
//! [`may_delete`] is the delete authorization rule on plain types. The server
//! reads an `APPLICATION` envelope's `Envelope.deletes` and its group kind and
//! calls it to authorise the delete (and the matching blob purge); the device
//! core calls it on a received `Content.Delete` before honouring it. Both sides
//! apply the same rule, so a delete the server accepts is the same a client
//! honours.

/// The authority that governs a group's deletes (Who may delete).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteAuthority {
    /// The lobby or a named group: only an admin device may delete, text or
    /// media, as moderation. A regular member may not delete even their own
    /// message.
    Group,
    /// A DM: only the target message's original sender may delete it. No device
    /// has more power; the admin is not a DM member.
    Dm,
}

/// Whether a delete is authorised (Who may delete).
///
/// In the lobby and a named group ([`DeleteAuthority::Group`]) the deleting
/// device must be an admin device (`is_admin`). In a DM
/// ([`DeleteAuthority::Dm`]) it must be the target message's original sender
/// (`is_target_sender`, i.e. the authenticated deleting device is the
/// `target_sender` the delete names). The server rejects an unauthorised delete
/// with `REJECTION_REASON_POLICY_VIOLATION` and a client ignores one.
pub fn may_delete(authority: DeleteAuthority, is_admin: bool, is_target_sender: bool) -> bool {
    match authority {
        DeleteAuthority::Group => is_admin,
        DeleteAuthority::Dm => is_target_sender,
    }
}
