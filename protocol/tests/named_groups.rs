//! External tests for the named-group rules in `protocol` (change
//! `lobby-multi-group`): the `0xF0A3` extension, the group kind, the named-group
//! commit authority rule and protocol version 3.
//!
//! Cast: the admin (`admin@chat`, phone at leaf 0), Bob (`bob@chat`, phone at
//! leaf 1) and Carol (`carol@chat`, phone at leaf 2) in the "Weekend" named group.

use protocol::group_membership::{
    ADMIN_EXTENSION_TYPE, CommitAuthorityError, Committer, DM_EXTENSION_TYPE, GroupKind,
    NAMED_EXTENSION_TYPE, Proposal, ProposalKind, ProposalSender, Violation,
    check_named_commit_authority, named_extension_data, parse_named_extension,
};
use protocol::wire_transport::{
    PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS, negotiate_protocol_version, server_knows_dms,
    server_knows_named_groups,
};

const ADMIN: &str = "admin@chat";
const BOB: &str = "bob@chat";
const CAROL: &str = "carol@chat";

fn admin() -> Committer<'static> {
    Committer {
        user: ADMIN,
        is_admin: true,
        leaf_index: 0,
    }
}

fn bob() -> Committer<'static> {
    Committer {
        user: BOB,
        is_admin: false,
        leaf_index: 1,
    }
}

fn by(leaf_index: u32, kind: ProposalKind<'static>) -> Proposal<'static> {
    Proposal {
        sender: ProposalSender::Member { leaf_index },
        kind,
    }
}

// spec: multi-group/named-group-creation
#[test]
fn the_named_extension_is_0xf0a3_and_distinct_from_the_others() {
    assert_eq!(NAMED_EXTENSION_TYPE, 0xF0A3);
    assert_ne!(NAMED_EXTENSION_TYPE, ADMIN_EXTENSION_TYPE);
    assert_ne!(NAMED_EXTENSION_TYPE, DM_EXTENSION_TYPE);
    assert!(parse_named_extension(&named_extension_data()));
    assert!(!parse_named_extension(b""));
    assert!(!parse_named_extension(b"other"));
}

// spec: group-membership/membership-state-on-the-server
#[test]
fn group_kind_round_trips_and_follows_the_extensions() {
    for kind in [GroupKind::System, GroupKind::Named, GroupKind::Dm] {
        assert_eq!(GroupKind::parse(kind.as_str()), Some(kind));
    }
    assert_eq!(GroupKind::parse("main"), None);
    assert_eq!(GroupKind::from_extensions(false, false), GroupKind::System);
    assert_eq!(GroupKind::from_extensions(false, true), GroupKind::Named);
    assert_eq!(GroupKind::from_extensions(true, false), GroupKind::Dm);
}

// spec: group-membership/commit-authority
#[test]
fn the_admin_moderates_a_named_group() {
    let add = by(0, ProposalKind::Add { target_user: BOB });
    let remove = by(0, ProposalKind::Remove { target_user: BOB });
    assert_eq!(
        check_named_commit_authority(&admin(), &[add, remove]),
        Ok(())
    );
}

// spec: group-membership/commit-authority
#[test]
fn a_member_cannot_moderate_a_named_group() {
    let add = by(1, ProposalKind::Add { target_user: CAROL });
    assert_eq!(
        check_named_commit_authority(&bob(), &[add]),
        Err(CommitAuthorityError {
            index: 0,
            violation: Violation::AddForAnotherUser
        })
    );
    let remove = by(1, ProposalKind::Remove { target_user: CAROL });
    assert_eq!(
        check_named_commit_authority(&bob(), &[remove]),
        Err(CommitAuthorityError {
            index: 0,
            violation: Violation::RemoveOfAnotherUser
        })
    );
    // Own devices and own leaf stay allowed.
    let own = [
        by(1, ProposalKind::Add { target_user: BOB }),
        by(1, ProposalKind::Update),
    ];
    assert_eq!(check_named_commit_authority(&bob(), &own), Ok(()));
}

// spec: wire-transport/protocol-version-negotiation
#[test]
fn version_3_with_a_new_server_and_2_with_an_old_one() {
    assert_eq!(PROTOCOL_VERSION, 3);
    assert_eq!(SUPPORTED_PROTOCOL_VERSIONS, [1, 2, 3]);
    assert_eq!(negotiate_protocol_version(&[1, 2, 3]), Some(3));
    assert_eq!(negotiate_protocol_version(&[1, 2]), Some(2));
    assert!(server_knows_named_groups(&[1, 2, 3]));
    assert!(!server_knows_named_groups(&[1, 2]));
    assert!(server_knows_dms(&[1, 2, 3]));
}
