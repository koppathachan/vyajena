//! External tests for `protocol::account_deletion` (public API only).

use protocol::account_deletion::may_delete_account;

// spec: account-deletion/the-admin-cannot-self-delete-through-the-rpc
#[test]
fn the_admin_cannot_self_delete_through_the_rpc() {
    // An admin caller is refused: the server answers FAILED_PRECONDITION and the
    // client never issues the RPC on an admin device.
    let is_admin = true;
    assert!(!may_delete_account(is_admin));
}

// spec: account-deletion/user-deletes-their-own-account
#[test]
fn an_ordinary_user_may_delete_their_own_account() {
    // Any non-admin caller may delete their own account; the RPC carries no
    // target id, so it can only ever be their own.
    let is_admin = false;
    assert!(may_delete_account(is_admin));
}
