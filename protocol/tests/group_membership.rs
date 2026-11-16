//! External tests for `protocol::group_membership` (public API only).
//!
//! Cast: the admin (`akhil@pi`, phone at leaf 0), Alice (`alice@pi`, phone at
//! leaf 1, tablet at leaf 2) and Bob (`bob@pi`, phone at leaf 3).

use protocol::group_membership::{
    CommitAuthorityError, Committer, Proposal, ProposalKind, ProposalSender, Violation,
    check_commit_authority,
};

const ADMIN: &str = "akhil@pi";
const ALICE: &str = "alice@pi";
const BOB: &str = "bob@pi";
const CAROL: &str = "carol@pi";

const ADMIN_PHONE: Committer<'static> = Committer {
    user: ADMIN,
    is_admin: true,
    leaf_index: 0,
};
const ALICE_PHONE: Committer<'static> = Committer {
    user: ALICE,
    is_admin: false,
    leaf_index: 1,
};
const BOB_PHONE: Committer<'static> = Committer {
    user: BOB,
    is_admin: false,
    leaf_index: 3,
};

/// A proposal carried by value, so its sender is the committer.
fn by_value<'a>(committer: &Committer<'_>, kind: ProposalKind<'a>) -> Proposal<'a> {
    Proposal {
        sender: ProposalSender::Member {
            leaf_index: committer.leaf_index,
        },
        kind,
    }
}

fn add(target_user: &str) -> ProposalKind<'_> {
    ProposalKind::Add { target_user }
}

fn remove(target_user: &str) -> ProposalKind<'_> {
    ProposalKind::Remove { target_user }
}

fn pi_remove(target_user: &str) -> Proposal<'_> {
    Proposal {
        sender: ProposalSender::Pi,
        kind: remove(target_user),
    }
}

fn rejected(index: usize, violation: Violation) -> Result<(), CommitAuthorityError> {
    Err(CommitAuthorityError { index, violation })
}

// ----- Allowed -----

// spec: group-membership/commit-authority
#[test]
fn member_links_their_own_device() {
    // Alice's phone commits an Add of Alice's newly linked tablet.
    let proposals = [by_value(&ALICE_PHONE, add(ALICE))];
    assert_eq!(check_commit_authority(&ALICE_PHONE, &proposals), Ok(()));
}

// spec: group-membership/commit-authority
#[test]
fn member_removes_their_own_device() {
    // Alice's phone removes Alice's lost tablet.
    let proposals = [by_value(&ALICE_PHONE, remove(ALICE))];
    assert_eq!(check_commit_authority(&ALICE_PHONE, &proposals), Ok(()));
}

// spec: group-membership/commit-authority
#[test]
fn member_updates_their_own_leaf() {
    // A commit with no proposals only updates the committer's leaf (its path).
    assert_eq!(check_commit_authority(&ALICE_PHONE, &[]), Ok(()));
    // An Update proposal whose sender is the committer's own leaf.
    let proposals = [by_value(&ALICE_PHONE, ProposalKind::Update)];
    assert_eq!(check_commit_authority(&ALICE_PHONE, &proposals), Ok(()));
}

// spec: group-membership/commit-authority
#[test]
fn member_commits_a_pi_issued_remove_of_another_user() {
    // group-membership: Inactive devices are kept out. Any member may commit the
    // Pi's external Remove, whoever it targets.
    assert_eq!(
        check_commit_authority(&ALICE_PHONE, &[pi_remove(BOB)]),
        Ok(())
    );
    assert_eq!(
        check_commit_authority(&ALICE_PHONE, &[pi_remove(ADMIN)]),
        Ok(())
    );
}

// spec: group-membership/commit-authority
#[test]
fn member_re_adds_their_own_returning_device() {
    // group-membership: Automatic re-add. Bob's tablet re-adds Bob's phone: the
    // Pi's pending Remove of the old leaf plus an Add of the fresh KeyPackage.
    let bob_tablet = Committer {
        user: BOB,
        is_admin: false,
        leaf_index: 4,
    };
    let proposals = [pi_remove(BOB), by_value(&bob_tablet, add(BOB))];
    assert_eq!(check_commit_authority(&bob_tablet, &proposals), Ok(()));
}

// spec: group-membership/commit-authority
#[test]
fn admin_adds_a_user() {
    // The admin's phone adds all of Carol's devices in one commit.
    let proposals = [
        by_value(&ADMIN_PHONE, add(CAROL)),
        by_value(&ADMIN_PHONE, add(CAROL)),
    ];
    assert_eq!(check_commit_authority(&ADMIN_PHONE, &proposals), Ok(()));
}

// spec: group-membership/commit-authority
#[test]
fn admin_commits_anything() {
    let bob_update = Proposal {
        sender: ProposalSender::Member { leaf_index: 3 },
        kind: ProposalKind::Update,
    };
    let other_external = Proposal {
        sender: ProposalSender::Other,
        kind: remove(BOB),
    };
    let proposals = [
        by_value(&ADMIN_PHONE, remove(BOB)),
        by_value(&ADMIN_PHONE, add(ALICE)),
        bob_update,
        pi_remove(ALICE),
        other_external,
        by_value(&ADMIN_PHONE, ProposalKind::Other { proposal_type: 7 }),
    ];
    assert_eq!(check_commit_authority(&ADMIN_PHONE, &proposals), Ok(()));
}

// spec: group-membership/commit-authority
#[test]
fn admin_re_adds_another_users_device() {
    let proposals = [pi_remove(BOB), by_value(&ADMIN_PHONE, add(BOB))];
    assert_eq!(check_commit_authority(&ADMIN_PHONE, &proposals), Ok(()));
}

// ----- Rejected -----

// spec: group-membership/commit-authority
#[test]
fn member_adds_someone_else() {
    // Alice's phone commits an Add of a device certified for Bob.
    let proposals = [by_value(&ALICE_PHONE, add(BOB))];
    assert_eq!(
        check_commit_authority(&ALICE_PHONE, &proposals),
        rejected(0, Violation::AddForAnotherUser)
    );
}

// spec: group-membership/commit-authority
#[test]
fn member_removes_someone_else() {
    // Alice's phone commits a Remove of one of Bob's devices.
    let proposals = [by_value(&ALICE_PHONE, remove(BOB))];
    assert_eq!(
        check_commit_authority(&ALICE_PHONE, &proposals),
        rejected(0, Violation::RemoveOfAnotherUser)
    );
}

// spec: group-membership/commit-authority
#[test]
fn member_commits_another_members_remove_proposal_of_someone_else() {
    // Bob proposed removing one of Alice's devices; Bob commits it himself: not
    // his user. Only Removes the Pi issued are exempt.
    let proposal = Proposal {
        sender: ProposalSender::Member { leaf_index: 0 },
        kind: remove(ALICE),
    };
    assert_eq!(
        check_commit_authority(&BOB_PHONE, &[proposal]),
        rejected(0, Violation::RemoveOfAnotherUser)
    );
}

// spec: group-membership/commit-authority
#[test]
fn member_commits_an_update_of_another_leaf() {
    // Alice's phone commits Bob's phone's Update proposal by reference.
    let proposal = Proposal {
        sender: ProposalSender::Member { leaf_index: 3 },
        kind: ProposalKind::Update,
    };
    assert_eq!(
        check_commit_authority(&ALICE_PHONE, &[proposal]),
        rejected(0, Violation::UpdateOfAnotherLeaf)
    );
    // Even the leaf of Alice's own tablet is another leaf.
    let tablet_update = Proposal {
        sender: ProposalSender::Member { leaf_index: 2 },
        kind: ProposalKind::Update,
    };
    assert_eq!(
        check_commit_authority(&ALICE_PHONE, &[tablet_update]),
        rejected(0, Violation::UpdateOfAnotherLeaf)
    );
}

// spec: group-membership/commit-authority
#[test]
fn member_commits_any_other_proposal_kind() {
    // PreSharedKey (4), ReInit (5), ExternalInit (6), GroupContextExtensions (7)
    // and custom types are admin-only.
    for proposal_type in [4, 5, 6, 7, 0xf000] {
        let proposals = [by_value(
            &ALICE_PHONE,
            ProposalKind::Other { proposal_type },
        )];
        assert_eq!(
            check_commit_authority(&ALICE_PHONE, &proposals),
            rejected(0, Violation::NotAllowed),
            "proposal type {proposal_type}"
        );
    }
}

// spec: group-membership/commit-authority
#[test]
fn member_commits_a_remove_from_another_external_sender() {
    let proposal = Proposal {
        sender: ProposalSender::Other,
        kind: remove(BOB),
    };
    assert_eq!(
        check_commit_authority(&ALICE_PHONE, &[proposal]),
        rejected(0, Violation::NotAllowed)
    );
}

// spec: group-membership/commit-authority
#[test]
fn pi_issued_proposals_other_than_remove_are_not_exempt() {
    for kind in [
        add(BOB),
        ProposalKind::Update,
        ProposalKind::Other { proposal_type: 7 },
    ] {
        let proposal = Proposal {
            sender: ProposalSender::Pi,
            kind,
        };
        assert_eq!(
            check_commit_authority(&ALICE_PHONE, &[proposal]),
            rejected(0, Violation::NotAllowed),
            "{kind:?}"
        );
    }
}

// spec: group-membership/commit-authority
#[test]
fn one_bad_proposal_rejects_the_whole_commit() {
    // Carol re-adds Bob's phone: the Pi's Remove is fine, the Add for Bob is not.
    let carol_phone = Committer {
        user: CAROL,
        is_admin: false,
        leaf_index: 5,
    };
    let proposals = [
        pi_remove(BOB),
        by_value(&carol_phone, ProposalKind::Update),
        by_value(&carol_phone, add(BOB)),
    ];
    assert_eq!(
        check_commit_authority(&carol_phone, &proposals),
        rejected(2, Violation::AddForAnotherUser)
    );
}

// spec: group-membership/commit-authority
#[test]
fn user_ids_compare_exactly() {
    // `alice@pi` and `alice@pi2` or `Alice@pi` are different users.
    for other in ["alice@pi2", "Alice@pi", "alice"] {
        let proposals = [by_value(&ALICE_PHONE, add(other))];
        assert_eq!(
            check_commit_authority(&ALICE_PHONE, &proposals),
            rejected(0, Violation::AddForAnotherUser),
            "{other}"
        );
    }
}

// spec: group-membership/group-creation
#[test]
fn admin_extension_is_private_use_and_holds_the_user_id() {
    use protocol::group_membership::{ADMIN_EXTENSION_TYPE, admin_extension_data};
    // RFC 9420 section 17.3: 0xF000-0xFFFF is reserved for private use.
    const { assert!(ADMIN_EXTENSION_TYPE >= 0xF000) };
    assert_eq!(admin_extension_data("akhil@pi"), b"akhil@pi".to_vec());
}

// spec: group-membership/own-leaf-rotation
#[test]
fn own_leaf_rotates_after_seven_days() {
    use protocol::group_membership::{
        INACTIVITY_TIMEOUT, OWN_LEAF_MAX_AGE, ROTATION_CHECK_INTERVAL, ROTATION_MAX_JITTER,
        own_leaf_rotation_due,
    };
    use std::time::Duration;
    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    assert_eq!(OWN_LEAF_MAX_AGE, 7 * DAY);
    // Same week as the inactivity removal (decision #4).
    assert_eq!(OWN_LEAF_MAX_AGE, INACTIVITY_TIMEOUT);
    // Quiet group: untouched for 7 days is due; a day less is not.
    assert!(own_leaf_rotation_due(7 * DAY));
    assert!(own_leaf_rotation_due(30 * DAY));
    assert!(!own_leaf_rotation_due(7 * DAY - Duration::from_secs(1)));
    assert!(!own_leaf_rotation_due(Duration::ZERO));
    // Checked at least daily, with up to an hour of jitter, so a rotation is
    // never more than a day and an hour late while the device is online.
    assert_eq!(ROTATION_CHECK_INTERVAL, DAY);
    assert_eq!(ROTATION_MAX_JITTER, Duration::from_secs(60 * 60));
    const { assert!(ROTATION_MAX_JITTER.as_secs() < ROTATION_CHECK_INTERVAL.as_secs()) };
}

// spec: group-membership/re-add-after-an-unusable-welcome
#[test]
fn readd_reason_is_cut_at_a_character_boundary() {
    use protocol::group_membership::{MAX_READD_REASON_BYTES, readd_reason};
    assert_eq!(
        readd_reason("no matching KeyPackage"),
        "no matching KeyPackage"
    );
    let long = "é".repeat(MAX_READD_REASON_BYTES); // 2 bytes each
    let cut = readd_reason(&long);
    assert!(cut.len() <= MAX_READD_REASON_BYTES);
    assert_eq!(cut.len(), MAX_READD_REASON_BYTES);
    let odd = format!("x{long}");
    let cut = readd_reason(&odd);
    assert_eq!(cut.len(), MAX_READD_REASON_BYTES - 1);
    assert!(odd.starts_with(cut));
}

// spec: group-membership/re-add-after-an-unusable-welcome
#[test]
fn readd_limits_are_a_few_a_day() {
    use protocol::group_membership::{MAX_READD_REQUESTS_PER_DAY, READD_REQUEST_WINDOW};
    assert_eq!(MAX_READD_REQUESTS_PER_DAY, 3);
    assert_eq!(READD_REQUEST_WINDOW.as_secs(), 24 * 3600);
}
