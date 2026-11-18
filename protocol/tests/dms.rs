//! External tests for the DM rules in `protocol` (change `dms`): the DM
//! extension, the DM commit authority rule and the protocol versions.
//!
//! Cast: Alice (`alice@pi`, phone at leaf 0, tablet at leaf 2), Bob (`bob@pi`,
//! phone at leaf 1) in the Alice–Bob DM, and Carol (`carol@pi`) outside it.

use protocol::group_membership::{
    CommitAuthorityError, Committer, DM_EXTENSION_TYPE, Proposal, ProposalKind, ProposalSender,
    Violation, check_dm_commit_authority, dm_extension_data, parse_dm_extension,
};
use protocol::wire_transport::{
    PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS, negotiate_protocol_version, pi_knows_dms,
};

const ALICE: &str = "alice@pi";
const BOB: &str = "bob@pi";
const CAROL: &str = "carol@pi";
const PAIR: (&str, &str) = (ALICE, BOB);

fn phone(user: &'static str, leaf_index: u32) -> Committer<'static> {
    Committer {
        user,
        is_admin: false,
        leaf_index,
    }
}

fn by(leaf_index: u32, kind: ProposalKind<'static>) -> Proposal<'static> {
    Proposal {
        sender: ProposalSender::Member { leaf_index },
        kind,
    }
}

fn add(user: &'static str) -> ProposalKind<'static> {
    ProposalKind::Add { target_user: user }
}

fn remove(user: &'static str) -> ProposalKind<'static> {
    ProposalKind::Remove { target_user: user }
}

// spec: direct-messages/opening-a-dm
#[test]
fn the_dm_extension_names_the_pair_in_either_order() {
    assert_eq!(DM_EXTENSION_TYPE, 0xF0A2);
    let data = dm_extension_data(BOB, ALICE);
    assert_eq!(data, dm_extension_data(ALICE, BOB));
    assert_eq!(data, b"alice@pi\nbob@pi");
    assert_eq!(
        parse_dm_extension(&data),
        Some((ALICE.to_owned(), BOB.to_owned()))
    );
}

// spec: direct-messages/opening-a-dm
#[test]
fn a_garbled_dm_extension_names_nobody() {
    for bad in [
        &b""[..],
        b"alice@pi",
        b"bob@pi\nalice@pi",
        b"alice@pi\nalice@pi",
        b"alice@pi\n",
        b"alice@pi\nbob@pi\ncarol@pi",
        b"alice@pi\nbob @pi",
        b"\xff\nbob@pi",
    ] {
        assert_eq!(parse_dm_extension(bad), None, "{bad:?}");
    }
}

// spec: group-membership/commit-authority
#[test]
fn either_user_adds_devices_of_either_user() {
    let alice = phone(ALICE, 0);
    let bob = phone(BOB, 1);
    assert_eq!(
        check_dm_commit_authority(&alice, PAIR, &[by(0, add(BOB)), by(0, add(ALICE))]),
        Ok(())
    );
    assert_eq!(
        check_dm_commit_authority(&bob, PAIR, &[by(1, add(ALICE))]),
        Ok(())
    );
}

// spec: group-membership/commit-authority
// spec: direct-messages/dm-members
#[test]
fn nobody_adds_a_third_user() {
    assert_eq!(
        check_dm_commit_authority(
            &phone(ALICE, 0),
            PAIR,
            &[by(0, add(BOB)), by(0, add(CAROL))]
        ),
        Err(CommitAuthorityError {
            index: 1,
            violation: Violation::AddForAnotherUser
        })
    );
}

// spec: group-membership/commit-authority
#[test]
fn only_own_devices_are_removed_except_by_the_pi() {
    let alice = phone(ALICE, 0);
    assert_eq!(
        check_dm_commit_authority(&alice, PAIR, &[by(0, remove(ALICE))]),
        Ok(())
    );
    assert_eq!(
        check_dm_commit_authority(&alice, PAIR, &[by(0, remove(BOB))]),
        Err(CommitAuthorityError {
            index: 0,
            violation: Violation::RemoveOfAnotherUser
        })
    );
    let pi_remove = Proposal {
        sender: ProposalSender::Pi,
        kind: remove(BOB),
    };
    assert_eq!(
        check_dm_commit_authority(&alice, PAIR, &[pi_remove]),
        Ok(())
    );
}

// spec: group-membership/commit-authority
#[test]
fn an_admin_has_no_extra_power_in_a_dm() {
    let admin_in_dm = Committer {
        user: ALICE,
        is_admin: true,
        leaf_index: 0,
    };
    assert_eq!(
        check_dm_commit_authority(&admin_in_dm, PAIR, &[by(0, remove(BOB))]),
        Err(CommitAuthorityError {
            index: 0,
            violation: Violation::RemoveOfAnotherUser
        })
    );
    let other = Proposal {
        sender: ProposalSender::Member { leaf_index: 0 },
        kind: ProposalKind::Other { proposal_type: 7 },
    };
    assert!(check_dm_commit_authority(&admin_in_dm, PAIR, &[other]).is_err());
}

// spec: group-membership/commit-authority
#[test]
fn own_updates_pass_and_others_do_not() {
    let tablet = phone(ALICE, 2);
    assert_eq!(
        check_dm_commit_authority(&tablet, PAIR, &[by(2, ProposalKind::Update)]),
        Ok(())
    );
    assert_eq!(
        check_dm_commit_authority(&tablet, PAIR, &[by(1, ProposalKind::Update)]),
        Err(CommitAuthorityError {
            index: 0,
            violation: Violation::UpdateOfAnotherLeaf
        })
    );
    assert_eq!(check_dm_commit_authority(&tablet, PAIR, &[]), Ok(()));
}

// spec: direct-messages/dm-members
#[test]
fn a_committer_outside_the_pair_is_refused() {
    assert_eq!(
        check_dm_commit_authority(&phone(CAROL, 3), PAIR, &[]),
        Err(CommitAuthorityError {
            index: 0,
            violation: Violation::NotAllowed
        })
    );
}

// spec: wire-transport/protocol-version-negotiation
#[test]
fn version_2_with_a_new_pi_and_1_with_an_old_one() {
    assert_eq!(PROTOCOL_VERSION, 2);
    assert_eq!(SUPPORTED_PROTOCOL_VERSIONS, [1, 2]);
    assert_eq!(negotiate_protocol_version(&[1, 2]), Some(2));
    assert_eq!(negotiate_protocol_version(&[1]), Some(1));
    assert_eq!(negotiate_protocol_version(&[3]), None);
    assert!(pi_knows_dms(&[1, 2]));
    assert!(!pi_knows_dms(&[1]));
}
