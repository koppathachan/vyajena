//! group-membership: Device-level membership (the Pi's mirror of the client
//! check); Group creation; Commit authority; Pi validates handshakes without
//! group secrets; Removing a user; Membership state on the Pi.
//!
//! `Admin.CreateGroup` and `Admin.RemoveUser` run over gRPC. The commit checks
//! run through `server::membership::GroupState` on the store's writer, as the
//! messaging write path (pi-server-core task 7.3) will call them.

mod support;

use openmls::prelude::CredentialWithKey;
use openmls::prelude::tls_codec::Serialize as _;
use openmls_basic_credential::SignatureKeyPair;
use server::auth::Caller;
use server::membership::{AcceptedCommit, GroupState, HandshakeRejection};
use support::clients::{CIPHERSUITE, TestDevice, TestUser};
use support::harness::{Conn, TestPi, authed};
use support::{Check, bootstrap_admin, invite_user, link_device, pb, pi_external_sender};
use tonic::Code;

const GROUP_ID: &[u8] = b"the-one-group";

fn caller(device: &TestDevice) -> Caller {
    Caller {
        device_id: device.device_id,
        username: device.username.clone(),
        user_id: format!("{}@pi", device.username),
        is_admin: device.username == "akhil",
    }
}

/// Runs a commit through the Pi's checks and, if accepted, saves the new state.
async fn submit(
    pi: &TestPi,
    device: &TestDevice,
    commit: &[u8],
) -> Result<AcceptedCommit, HandshakeRejection> {
    let caller = caller(device);
    let commit = commit.to_vec();
    pi.state
        .store
        .call(move |conn| {
            let tx = conn.transaction()?;
            let Some(mut group) = GroupState::load(&tx)? else {
                panic!("no group");
            };
            let result = group.check_commit(&tx, &caller, "pi", &commit)?;
            if result.is_ok() {
                group.save(&tx)?;
                tx.commit()?;
            }
            Ok(result)
        })
        .await
        .check("store")
}

/// Runs a proposal through the Pi's checks and saves it.
async fn submit_proposal(
    pi: &TestPi,
    device: &TestDevice,
    proposal: &[u8],
) -> Result<(), HandshakeRejection> {
    let caller = caller(device);
    let proposal = proposal.to_vec();
    pi.state
        .store
        .call(move |conn| {
            let tx = conn.transaction()?;
            let Some(mut group) = GroupState::load(&tx)? else {
                panic!("no group");
            };
            let result = group.check_proposal(&tx, &caller, &proposal)?;
            if result.is_ok() {
                group.save(&tx)?;
                tx.commit()?;
            }
            Ok(result)
        })
        .await
        .check("store")
}

/// The Pi's ratchet tree, TLS-serialized.
async fn pi_tree(pi: &TestPi) -> Vec<u8> {
    pi.state
        .store
        .call(|conn| {
            let group = GroupState::load(conn)?.check("group");
            Ok(group
                .public_group()
                .export_ratchet_tree()
                .tls_serialize_detached()
                .check("tree"))
        })
        .await
        .check("store")
}

/// The admin's device creates the group over gRPC.
async fn create_group(conn: &Conn, admin: &mut TestDevice) {
    let pi_sender = pi_external_sender(conn).await;
    let request = admin.create_group(GROUP_ID, &pi_sender, "akhil@pi");
    conn.admin()
        .create_group(authed(admin.token(), request))
        .await
        .check("CreateGroup");
}

/// A fresh one-time KeyPackage of each of `user`'s devices, via the Pi.
async fn fetch_all(conn: &Conn, caller: &TestDevice, user: &TestUser) -> Vec<Vec<u8>> {
    conn.keys()
        .fetch_key_packages(authed(
            caller.token(),
            pb::FetchKeyPackagesRequest {
                user_id: Some(user.pb_user_id()),
                device_ids: vec![],
                group_id: Vec::new(),
            },
        ))
        .await
        .check("FetchKeyPackages")
        .into_inner()
        .devices
        .into_iter()
        .map(|d| d.key_package)
        .collect()
}

/// A fresh KeyPackage of one of `user`'s devices, via the Pi.
async fn fetch_for(
    conn: &Conn,
    caller: &TestDevice,
    user: &TestUser,
    device: &TestDevice,
) -> Vec<Vec<u8>> {
    conn.keys()
        .fetch_key_packages(authed(
            caller.token(),
            pb::FetchKeyPackagesRequest {
                user_id: Some(user.pb_user_id()),
                device_ids: vec![device.device_id.to_vec()],
                group_id: Vec::new(),
            },
        ))
        .await
        .check("FetchKeyPackages")
        .into_inner()
        .devices
        .into_iter()
        .map(|d| d.key_package)
        .collect()
}

/// `committer` adds the given KeyPackages; the Pi accepts; everyone in
/// `others` processes the commit and the new devices join.
async fn add_accepted(
    pi: &TestPi,
    committer: &mut TestDevice,
    key_packages: &[Vec<u8>],
    others: &mut [&mut TestDevice],
    joiners: &mut [&mut TestDevice],
) -> AcceptedCommit {
    let (commit, welcome) = committer.add(key_packages);
    let accepted = submit(pi, committer, &commit)
        .await
        .check("commit accepted");
    committer.confirm();
    for other in others.iter_mut() {
        other.receive_commit(&commit);
    }
    for joiner in joiners.iter_mut() {
        joiner.join(&welcome);
    }
    accepted
}

/// Admin + group + Alice (laptop) in the group.
struct Setup {
    pi: TestPi,
    conn: Conn,
    admin: TestDevice,
    alice: TestUser,
    laptop: TestDevice,
}

async fn setup() -> Setup {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, mut admin) = bootstrap_admin(&pi, &conn).await;
    create_group(&conn, &mut admin).await;
    let (alice, mut laptop) = invite_user(&conn, &admin, "alice").await;
    laptop.upload_key_packages(&conn, 3).await;
    let key_packages = fetch_all(&conn, &admin, &alice).await;
    add_accepted(&pi, &mut admin, &key_packages, &mut [], &mut [&mut laptop]).await;
    Setup {
        pi,
        conn,
        admin,
        alice,
        laptop,
    }
}

// spec: group-membership/device-level-membership
#[tokio::test]
async fn genuine_certificate_on_the_wrong_key_is_a_policy_violation() {
    let Setup {
        pi,
        conn,
        mut admin,
        alice,
        mut laptop,
    } = setup().await;
    let mut tablet = link_device(&conn, &alice, &laptop).await;

    // Clients reject this commit; the Pi refuses to sequence it too (D10).
    let other = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).check("key");
    let swapped = tablet.key_package_signed_by(
        CIPHERSUITE,
        &other,
        CredentialWithKey {
            credential: tablet.credential_with_key().credential,
            signature_key: other.public().into(),
        },
    );
    let (commit, _) = admin.add(&[swapped]);
    let result = submit(&pi, &admin, &commit).await;
    assert!(
        matches!(result, Err(HandshakeRejection::PolicyViolation(_))),
        "{result:?}"
    );
    admin.abort();

    // The certificate on its own key: a member device, which makes Alice a
    // member through it as well as through her laptop.
    let genuine = tablet.key_package();
    let accepted = add_accepted(
        &pi,
        &mut admin,
        &[genuine],
        &mut [&mut laptop],
        &mut [&mut tablet],
    )
    .await;
    assert_eq!(accepted.added.len(), 1);
    assert_eq!(accepted.added[0].device_id, tablet.device_id);
}

// spec: group-membership/group-creation
#[tokio::test]
async fn admin_creates_the_group_at_epoch_0_as_its_only_member() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, mut admin) = bootstrap_admin(&pi, &conn).await;
    create_group(&conn, &mut admin).await;

    let (group_id, epoch): (Vec<u8>, i64) = pi
        .query(|db| {
            db.query_row("SELECT group_id, epoch FROM mls_group", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
        })
        .await;
    assert_eq!(group_id, GROUP_ID);
    assert_eq!(epoch, 0);
    let members = pi
        .state
        .store
        .call(|db| GroupState::members(db))
        .await
        .check("members");
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].device_id, admin.device_id);
    assert_eq!(members[0].leaf_index, 0);
    assert_eq!(
        pi_tree(&pi).await,
        admin
            .group()
            .export_ratchet_tree()
            .tls_serialize_detached()
            .check("tree")
    );
}

// spec: group-membership/group-creation
#[tokio::test]
async fn second_group_is_already_exists() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, mut admin) = bootstrap_admin(&pi, &conn).await;
    create_group(&conn, &mut admin).await;
    let pi_sender = pi_external_sender(&conn).await;
    let request = admin.create_group(b"another-group", &pi_sender, "akhil@pi");
    let result = conn
        .admin()
        .create_group(authed(admin.token(), request))
        .await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::AlreadyExists));
}

// spec: group-membership/group-creation
#[tokio::test]
async fn group_must_list_the_pi_name_the_admin_and_match_its_id() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, mut admin) = bootstrap_admin(&pi, &conn).await;
    let pi_sender = pi_external_sender(&conn).await;
    let invalid = |request: pb::CreateGroupRequest, token: String| {
        let mut client = conn.admin();
        async move {
            let result = client.create_group(authed(&token, request)).await;
            assert_eq!(result.err().map(|s| s.code()), Some(Code::InvalidArgument));
        }
    };
    let token = admin.token().to_owned();

    // Another external sender instead of the Pi.
    let mut stranger = pi_sender.clone();
    stranger.signature_public_key = support::clients::new_key()
        .verifying_key()
        .to_bytes()
        .to_vec();
    invalid(
        admin.create_group(b"g1", &stranger, "akhil@pi"),
        token.clone(),
    )
    .await;

    // Naming someone else as admin.
    invalid(
        admin.create_group(b"g2", &pi_sender, "alice@pi"),
        token.clone(),
    )
    .await;

    // group_id that does not match the GroupInfo.
    let mut request = admin.create_group(b"g3", &pi_sender, "akhil@pi");
    request.group_id = b"other".to_vec();
    invalid(request, token.clone()).await;

    // Garbage.
    let mut request = admin.create_group(b"g4", &pi_sender, "akhil@pi");
    request.ratchet_tree.truncate(10);
    invalid(request, token.clone()).await;
    invalid(pb::CreateGroupRequest::default(), token.clone()).await;

    let groups: i64 = pi
        .query(|db| db.query_row("SELECT count(*) FROM mls_group", [], |r| r.get(0)))
        .await;
    assert_eq!(groups, 0);
}

// spec: group-membership/group-creation
#[tokio::test]
async fn only_an_admin_device_creates_the_group_and_must_be_its_member() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (admin_user, mut admin) = bootstrap_admin(&pi, &conn).await;
    let pi_sender = pi_external_sender(&conn).await;
    let (_, mut alice) = invite_user(&conn, &admin, "alice").await;

    let request = alice.create_group(GROUP_ID, &pi_sender, "akhil@pi");
    let result = conn
        .admin()
        .create_group(authed(alice.token(), request))
        .await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::PermissionDenied));

    // The admin's tablet uploads a group whose member is the admin's phone.
    let tablet = link_device(&conn, &admin_user, &admin).await;
    let request = admin.create_group(GROUP_ID, &pi_sender, "akhil@pi");
    let result = conn
        .admin()
        .create_group(authed(tablet.token(), request))
        .await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::PermissionDenied));
}

// spec: group-membership/commit-authority
#[tokio::test]
async fn admin_adds_all_of_carols_devices_in_one_commit() {
    let Setup {
        pi,
        conn,
        mut admin,
        mut laptop,
        ..
    } = setup().await;
    let (carol, mut phone) = invite_user(&conn, &admin, "carol").await;
    let mut tablet = link_device(&conn, &carol, &phone).await;
    phone.upload_key_packages(&conn, 1).await;
    tablet.upload_key_packages(&conn, 1).await;
    let key_packages = fetch_all(&conn, &admin, &carol).await;
    assert_eq!(key_packages.len(), 2);

    let accepted = add_accepted(
        &pi,
        &mut admin,
        &key_packages,
        &mut [&mut laptop],
        &mut [&mut phone, &mut tablet],
    )
    .await;
    let mut added: Vec<_> = accepted.added.iter().map(|d| d.device_id).collect();
    added.sort();
    let mut expected = vec![phone.device_id, tablet.device_id];
    expected.sort();
    assert_eq!(added, expected, "the Welcome must be for exactly these");
    assert_eq!((accepted.from_epoch, accepted.epoch), (1, 2));
    // Everyone, the Pi included, has the same tree.
    let tree = pi_tree(&pi).await;
    for device in [&admin, &laptop, &phone, &tablet] {
        assert_eq!(
            device
                .group()
                .export_ratchet_tree()
                .tls_serialize_detached()
                .check("tree"),
            tree
        );
    }
}

// spec: group-membership/commit-authority
#[tokio::test]
async fn member_links_their_own_device() {
    let Setup {
        pi,
        conn,
        mut admin,
        alice,
        mut laptop,
    } = setup().await;
    let mut tablet = link_device(&conn, &alice, &laptop).await;
    tablet.upload_key_packages(&conn, 1).await;
    let key_packages = fetch_for(&conn, &laptop, &alice, &tablet).await;
    let accepted = add_accepted(
        &pi,
        &mut laptop,
        &key_packages,
        &mut [&mut admin],
        &mut [&mut tablet],
    )
    .await;
    assert_eq!(accepted.added.len(), 1);
    assert_eq!(accepted.added[0].device_id, tablet.device_id);
    assert_eq!(accepted.committer.device_id, laptop.device_id);
}

// spec: group-membership/commit-authority
#[tokio::test]
async fn member_adding_someone_else_is_a_policy_violation() {
    let Setup {
        pi,
        conn,
        admin,
        mut laptop,
        ..
    } = setup().await;
    let (bob, bobs_phone) = invite_user(&conn, &admin, "bob").await;
    bobs_phone.upload_key_packages(&conn, 1).await;
    let key_packages = fetch_all(&conn, &laptop, &bob).await;
    let (commit, _welcome) = laptop.add(&key_packages);
    let result = submit(&pi, &laptop, &commit).await;
    assert!(
        matches!(&result, Err(HandshakeRejection::PolicyViolation(why)) if why.contains("AddForAnotherUser")),
        "{result:?}"
    );
    assert_eq!(
        result.err().map(|r| r.reason()),
        Some(server::pb::RejectionReason::PolicyViolation)
    );
    laptop.abort();
}

// spec: group-membership/commit-authority
#[tokio::test]
async fn member_removing_someone_else_is_a_policy_violation() {
    let Setup {
        pi,
        conn,
        mut admin,
        mut laptop,
        ..
    } = setup().await;
    let (bob, mut bobs_phone) = invite_user(&conn, &admin, "bob").await;
    bobs_phone.upload_key_packages(&conn, 1).await;
    let key_packages = fetch_all(&conn, &admin, &bob).await;
    add_accepted(
        &pi,
        &mut admin,
        &key_packages,
        &mut [&mut laptop],
        &mut [&mut bobs_phone],
    )
    .await;

    let commit = laptop.remove(&[bobs_phone.leaf()]);
    let result = submit(&pi, &laptop, &commit).await;
    assert!(
        matches!(&result, Err(HandshakeRejection::PolicyViolation(why)) if why.contains("RemoveOfAnotherUser")),
        "{result:?}"
    );
}

// spec: group-membership/commit-authority
#[tokio::test]
async fn member_removes_their_own_device_and_updates_their_own_leaf() {
    let Setup {
        pi,
        conn,
        mut admin,
        alice,
        mut laptop,
    } = setup().await;
    let mut phone = link_device(&conn, &alice, &laptop).await;
    phone.upload_key_packages(&conn, 1).await;
    let key_packages = fetch_for(&conn, &laptop, &alice, &phone).await;
    add_accepted(
        &pi,
        &mut laptop,
        &key_packages,
        &mut [&mut admin],
        &mut [&mut phone],
    )
    .await;

    let update = laptop.self_update();
    submit(&pi, &laptop, &update).await.check("own leaf update");
    laptop.confirm();
    admin.receive_commit(&update);
    phone.receive_commit(&update);

    let remove = laptop.remove(&[phone.leaf()]);
    let accepted = submit(&pi, &laptop, &remove)
        .await
        .check("own device removed");
    assert_eq!(accepted.removed.len(), 1);
    assert_eq!(accepted.removed[0].device_id, phone.device_id);
    laptop.confirm();
    admin.receive_commit(&remove);
    assert_eq!(
        pi_tree(&pi).await,
        admin
            .group()
            .export_ratchet_tree()
            .tls_serialize_detached()
            .check("tree")
    );
}

// spec: group-membership/commit-authority
#[tokio::test]
async fn add_of_an_unregistered_device_is_a_policy_violation() {
    let Setup { pi, mut admin, .. } = setup().await;
    // A device the Pi has never seen, with a self-made identity.
    let ghost = TestUser::new("ghost").new_device();
    let (commit, _) = admin.add(&[ghost.key_package()]);
    let result = submit(&pi, &admin, &commit).await;
    assert!(
        matches!(result, Err(HandshakeRejection::PolicyViolation(_))),
        "{result:?}"
    );
}

// spec: group-membership/pi-validates-handshakes-without-group-secrets
#[tokio::test]
async fn commit_signed_by_another_leaf_is_a_policy_violation() {
    let Setup {
        pi,
        conn,
        mut admin,
        mut laptop,
        ..
    } = setup().await;
    let (bob, mut bobs_phone) = invite_user(&conn, &admin, "bob").await;
    bobs_phone.upload_key_packages(&conn, 1).await;
    let key_packages = fetch_all(&conn, &admin, &bob).await;
    add_accepted(
        &pi,
        &mut admin,
        &key_packages,
        &mut [&mut laptop],
        &mut [&mut bobs_phone],
    )
    .await;

    // Alice's laptop made this commit; Bob's phone sends it.
    let update = laptop.self_update();
    let result = submit(&pi, &bobs_phone, &update).await;
    assert!(
        matches!(&result, Err(HandshakeRejection::PolicyViolation(why)) if why.contains("sender leaf")),
        "{result:?}"
    );
    // The genuine sender is still accepted.
    submit(&pi, &laptop, &update).await.check("own commit");
}

// spec: group-membership/pi-validates-handshakes-without-group-secrets
#[tokio::test]
async fn commit_with_a_bad_signature_is_a_policy_violation() {
    let Setup { pi, mut laptop, .. } = setup().await;
    let update = laptop.self_update();
    // PublicMessage commit from a member ends with the signature (64 bytes),
    // then the confirmation tag and the membership tag (1 + 32 bytes each).
    let mut forged = update.clone();
    let last_signature_byte = forged.len() - 2 * 33 - 1;
    forged[last_signature_byte] ^= 0x01;
    let result = submit(&pi, &laptop, &forged).await;
    assert!(
        matches!(&result, Err(HandshakeRejection::PolicyViolation(why)) if why.contains("signature")),
        "{result:?}"
    );
    submit(&pi, &laptop, &update).await.check("unforged commit");
}

// spec: group-membership/pi-validates-handshakes-without-group-secrets
#[tokio::test]
async fn non_member_stale_and_malformed_commits_are_rejected() {
    let Setup {
        pi,
        conn,
        mut admin,
        mut laptop,
        ..
    } = setup().await;
    // A registered device that is not in the group.
    let (_, outsider) = invite_user(&conn, &admin, "bob").await;
    let update = laptop.self_update();
    let result = submit(&pi, &outsider, &update).await;
    assert!(
        matches!(result, Err(HandshakeRejection::PolicyViolation(_))),
        "{result:?}"
    );

    let result = submit(&pi, &laptop, b"not MLS").await;
    assert!(
        matches!(result, Err(HandshakeRejection::Malformed(_))),
        "{result:?}"
    );

    // The laptop's update is accepted; the admin's commit from the old epoch
    // is then stale.
    let admins = admin.self_update();
    submit(&pi, &laptop, &update).await.check("laptop update");
    let result = submit(&pi, &admin, &admins).await;
    assert_eq!(
        result,
        Err(HandshakeRejection::StaleEpoch {
            sent: 1,
            current: 2
        })
    );
    assert_eq!(result.err().and_then(|r| r.current_epoch()), Some(2));
}

// spec: group-membership/pi-validates-handshakes-without-group-secrets
#[tokio::test]
async fn proposals_are_kept_for_a_later_commit_by_reference() {
    let Setup {
        pi,
        conn,
        mut admin,
        alice,
        mut laptop,
    } = setup().await;
    let mut phone = link_device(&conn, &alice, &laptop).await;
    phone.upload_key_packages(&conn, 1).await;
    let key_packages = fetch_for(&conn, &laptop, &alice, &phone).await;
    add_accepted(
        &pi,
        &mut laptop,
        &key_packages,
        &mut [&mut admin],
        &mut [&mut phone],
    )
    .await;

    // The laptop proposes removing the phone; the admin commits it by reference.
    let proposal = laptop.propose_remove(phone.leaf());
    submit_proposal(&pi, &laptop, &proposal)
        .await
        .check("proposal");
    admin.receive_proposal(&proposal);
    let commit = admin.commit_pending();
    let accepted = submit(&pi, &admin, &commit)
        .await
        .check("commit by reference");
    assert_eq!(accepted.removed.len(), 1);
    assert_eq!(accepted.removed[0].device_id, phone.device_id);
}

// spec: group-membership/removing-a-user
#[tokio::test]
async fn removed_user_fails_authentication_and_is_no_longer_a_member() {
    let Setup {
        pi,
        conn,
        mut admin,
        mut laptop,
        ..
    } = setup().await;
    let (bob, mut phone) = invite_user(&conn, &admin, "bob").await;
    let mut tablet = link_device(&conn, &bob, &phone).await;
    phone.upload_key_packages(&conn, 1).await;
    tablet.upload_key_packages(&conn, 1).await;
    let key_packages = fetch_all(&conn, &admin, &bob).await;
    add_accepted(
        &pi,
        &mut admin,
        &key_packages,
        &mut [&mut laptop],
        &mut [&mut phone, &mut tablet],
    )
    .await;

    // The admin commits Removes of all of Bob's devices, then calls RemoveUser.
    let commit = admin.remove(&[phone.leaf(), tablet.leaf()]);
    submit(&pi, &admin, &commit)
        .await
        .check("remove Bob's devices");
    admin.confirm();
    let mut revocations = pi.state.revocations.subscribe();
    conn.admin()
        .remove_user(authed(
            admin.token(),
            pb::RemoveUserRequest {
                user_id: Some(bob.pb_user_id()),
            },
        ))
        .await
        .check("RemoveUser");

    for device in [&phone, &tablet] {
        let session = conn
            .devices()
            .get_self(authed(device.token(), pb::GetSelfRequest {}))
            .await;
        assert_eq!(session.err().map(|s| s.code()), Some(Code::Unauthenticated));
        // A valid signature from a revoked device: PERMISSION_DENIED.
        let auth = device.try_authenticate_with(&conn, &conn.exporter).await;
        assert_eq!(auth.err().map(|s| s.code()), Some(Code::PermissionDenied));
    }
    let mut closed = vec![
        revocations.try_recv().check("revocation 1"),
        revocations.try_recv().check("revocation 2"),
    ];
    closed.sort();
    let mut expected = vec![phone.device_id, tablet.device_id];
    expected.sort();
    assert_eq!(closed, expected);

    let members = pi
        .state
        .store
        .call(|db| GroupState::members(db))
        .await
        .check("members");
    assert!(members.iter().all(|m| m.username != "bob"));
}

// spec: group-membership/removing-a-user
#[tokio::test]
async fn admin_resets_a_user_who_lost_their_identity_device() {
    let Setup {
        pi,
        conn,
        mut admin,
        mut laptop,
        ..
    } = setup().await;
    let (old_bob, mut old_phone) = invite_user(&conn, &admin, "bob").await;
    old_phone.upload_key_packages(&conn, 1).await;
    let key_packages = fetch_all(&conn, &admin, &old_bob).await;
    add_accepted(
        &pi,
        &mut admin,
        &key_packages,
        &mut [&mut laptop],
        &mut [&mut old_phone],
    )
    .await;

    // Bob lost his phone: the admin removes him from the group and the Pi.
    let commit = admin.remove(&[old_phone.leaf()]);
    submit(&pi, &admin, &commit)
        .await
        .check("remove Bob's phone");
    admin.confirm();
    laptop.receive_commit(&commit);
    conn.admin()
        .remove_user(authed(
            admin.token(),
            pb::RemoveUserRequest {
                user_id: Some(old_bob.pb_user_id()),
            },
        ))
        .await
        .check("RemoveUser");

    // The username is free again: the Pi issues a new invite for bob@pi.
    let invite = support::create_invite(&conn, &admin, "bob").await;
    let invite = invite.invite.check("invite");
    assert_eq!(invite.user_id.check("user id").username, "bob");

    // Bob's new phone registers as bob@pi with a new identity key.
    let new_bob = TestUser::new("bob");
    assert_ne!(new_bob.identity_public_key(), old_bob.identity_public_key());
    let mut new_phone = new_bob.new_device();
    let registered = support::register(&conn, &invite.code, &new_bob, &new_phone)
        .await
        .check("Register after reset");
    assert_eq!(registered.user_id, Some(new_bob.pb_user_id()));
    new_phone.authenticate(&conn).await;
    new_phone.upload_key_packages(&conn, 1).await;

    // Members see the changed identity key in the fetch, and the old phone
    // stays locked out.
    let fetched = conn
        .keys()
        .fetch_key_packages(authed(
            admin.token(),
            pb::FetchKeyPackagesRequest {
                user_id: Some(new_bob.pb_user_id()),
                device_ids: vec![],
                group_id: Vec::new(),
            },
        ))
        .await
        .check("FetchKeyPackages")
        .into_inner();
    assert_eq!(fetched.identity_public_key, new_bob.identity_public_key());
    assert_eq!(fetched.devices.len(), 1, "only the new phone");
    let old = old_phone.try_authenticate_with(&conn, &conn.exporter).await;
    assert_eq!(old.err().map(|s| s.code()), Some(Code::PermissionDenied));

    // The admin adds the new phone back into the group.
    let key_packages: Vec<_> = fetched.devices.into_iter().map(|d| d.key_package).collect();
    let accepted = add_accepted(
        &pi,
        &mut admin,
        &key_packages,
        &mut [&mut laptop],
        &mut [&mut new_phone],
    )
    .await;
    assert_eq!(accepted.added.len(), 1);
    assert_eq!(accepted.added[0].device_id, new_phone.device_id);

    // A second invite while the new Bob exists is refused again.
    let again = conn
        .admin()
        .create_invite(authed(
            admin.token(),
            pb::CreateInviteRequest {
                username: "bob".to_owned(),
            },
        ))
        .await;
    assert_eq!(again.err().map(|s| s.code()), Some(Code::AlreadyExists));
}

// spec: group-membership/removing-a-user
#[tokio::test]
async fn only_the_admin_removes_users_and_not_the_admin() {
    let Setup {
        pi: _pi,
        conn,
        admin,
        laptop,
        ..
    } = setup().await;
    let remove = |token: String, user: pb::UserId| {
        let mut client = conn.admin();
        async move {
            client
                .remove_user(authed(
                    &token,
                    pb::RemoveUserRequest {
                        user_id: Some(user),
                    },
                ))
                .await
                .err()
                .map(|s| s.code())
        }
    };
    let admin_id = TestUser::new("akhil").pb_user_id();
    assert_eq!(
        remove(laptop.token().to_owned(), admin_id.clone()).await,
        Some(Code::PermissionDenied)
    );
    assert_eq!(
        remove(admin.token().to_owned(), admin_id).await,
        Some(Code::FailedPrecondition)
    );
    assert_eq!(
        remove(
            admin.token().to_owned(),
            TestUser::new("nobody").pb_user_id()
        )
        .await,
        Some(Code::NotFound)
    );
}

// spec: group-membership/removing-a-user
#[tokio::test]
async fn old_invite_dies_with_the_removal() {
    let Setup {
        pi: _pi,
        conn,
        admin,
        ..
    } = setup().await;
    // Two invites for bob@pi: Bob signs up with one, the other stays unused.
    let spare = support::create_invite(&conn, &admin, "bob")
        .await
        .invite
        .check("spare invite")
        .code;
    let (bob, _phone) = invite_user(&conn, &admin, "bob").await;

    conn.admin()
        .remove_user(authed(
            admin.token(),
            pb::RemoveUserRequest {
                user_id: Some(bob.pb_user_id()),
            },
        ))
        .await
        .check("RemoveUser");

    // The old invite is gone from the list and no longer registers anyone.
    let listed = conn
        .admin()
        .list_invites(authed(admin.token(), pb::ListInvitesRequest {}))
        .await
        .check("ListInvites")
        .into_inner();
    assert!(listed.invites.iter().all(|i| i.code != spare));
    let intruder = TestUser::new("bob");
    let old = support::register(&conn, &spare, &intruder, &intruder.new_device()).await;
    assert_eq!(old.err().map(|s| s.code()), Some(Code::PermissionDenied));

    // Only an invite created after the removal works.
    let fresh = support::create_invite(&conn, &admin, "bob")
        .await
        .invite
        .check("fresh invite")
        .code;
    let new_bob = TestUser::new("bob");
    let registered = support::register(&conn, &fresh, &new_bob, &new_bob.new_device())
        .await
        .check("Register with the new invite");
    assert_eq!(registered.user_id, Some(new_bob.pb_user_id()));
}

// spec: group-membership/membership-state-on-the-pi
#[tokio::test]
async fn pi_stores_only_ids_epoch_leaves_tree_context_and_no_names() {
    let Setup {
        pi, laptop, admin, ..
    } = setup().await;
    let tables: Vec<String> = pi
        .query(|db| {
            let mut stmt = db.prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'mls_%' ORDER BY name",
            )?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect()
        })
        .await;
    assert_eq!(tables, vec!["mls_group", "mls_members", "mls_public_state"]);
    let columns: Vec<String> = pi
        .query(|db| {
            let mut stmt = db.prepare(
                "SELECT name FROM pragma_table_info('mls_group')
                 UNION ALL SELECT name FROM pragma_table_info('mls_members')
                 UNION ALL SELECT name FROM pragma_table_info('mls_public_state')",
            )?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect()
        })
        .await;
    assert!(
        columns
            .iter()
            .all(|c| !c.contains("name") && !c.contains("setting")),
        "{columns:?}"
    );
    let members = pi
        .state
        .store
        .call(|db| GroupState::members(db))
        .await
        .check("members");
    let leaves: Vec<_> = members
        .iter()
        .map(|m| (m.leaf_index, m.device_id))
        .collect();
    assert_eq!(
        leaves,
        vec![
            (admin.leaf().u32(), admin.device_id),
            (laptop.leaf().u32(), laptop.device_id)
        ]
    );
}
