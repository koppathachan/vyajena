//! account-deletion: who may delete an account (User deletes their own account;
//! The admin cannot self-delete through the RPC).
//!
//! `Devices.DeleteAccount` carries no target user id, so it always deletes the
//! authenticated caller's own user — it can never be pointed at another account,
//! and there is no "delete anyone" power to guard. The one rule is the admin:
//! the admin account cannot be self-deleted through the RPC, because bootstrap
//! is possible only on an empty-users database, single-device admin key-loss is
//! a permanent lockout, and the lobby's only committer is the admin. The admin's
//! only reset path is the operator-only, host-gated `server admin-reset`
//! (admin-recovery: Reset needs genuine host access).
//!
//! The server reads the caller's user from its session token and calls
//! [`may_delete_account`] with whether that user is the admin; the device core
//! calls the same rule before issuing the RPC so an admin device never asks.
//! Both sides apply the same rule, so a delete the server would refuse is one a
//! client never sends.

/// Whether the authenticated caller may delete their own account through
/// `Devices.DeleteAccount`.
///
/// The RPC always targets the caller's own user, so the only restriction is the
/// admin: an admin caller is refused (the server answers `FAILED_PRECONDITION`
/// and deletes nothing; the client does not call the RPC on an admin device).
/// Every non-admin caller may delete their own account.
pub fn may_delete_account(caller_is_admin: bool) -> bool {
    !caller_is_admin
}
