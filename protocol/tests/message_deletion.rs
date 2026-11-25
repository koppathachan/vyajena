//! External tests for `protocol::message_deletion` (public API only).

use protocol::message_deletion::{DeleteAuthority, may_delete};

// spec: message-deletion/who-may-delete
#[test]
fn admin_deletes_in_a_group() {
    // In the lobby or a named group only an admin device may delete, whoever
    // sent the message.
    let admin = true;
    assert!(may_delete(DeleteAuthority::Group, admin, false));
    assert!(may_delete(DeleteAuthority::Group, admin, true));
}

// spec: message-deletion/who-may-delete
#[test]
fn member_cannot_delete_in_a_group_even_their_own() {
    // A regular member may not delete in a group, even a message it sent.
    let admin = false;
    assert!(!may_delete(DeleteAuthority::Group, admin, false));
    assert!(!may_delete(DeleteAuthority::Group, admin, true));
}

// spec: message-deletion/who-may-delete
#[test]
fn sender_deletes_their_own_dm_message() {
    // In a DM only the target's original sender may delete it; being admin is
    // irrelevant.
    assert!(may_delete(DeleteAuthority::Dm, false, true));
    assert!(may_delete(DeleteAuthority::Dm, true, true));
}

// spec: message-deletion/who-may-delete
#[test]
fn no_one_else_deletes_in_a_dm() {
    // A delete in a DM from a device that is not the target's sender is refused,
    // admin or not.
    assert!(!may_delete(DeleteAuthority::Dm, false, false));
    assert!(!may_delete(DeleteAuthority::Dm, true, false));
}
