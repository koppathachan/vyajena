//! key-directory: Clients verify the identity chain; Identity key lookup, end
//! to end against the real server: identity keys pinned on first sight (looked up
//! with `Keys.GetIdentityKeys`, consuming no KeyPackages), a changed key
//! blocks the admin adding that user's devices until the admin accepts it,
//! and members follow the admin's re-add of a reset user (client-core tasks
//! 5.2, 5.5, 5.6; design D5).

mod support;

use client::{ClientError, Event};
use support::Check;
use support::device::{self, Device, member, user};
use support::server::TestServer;

// spec: key-directory/clients-verify-the-identity-chain
// spec: group-membership/removing-a-user
#[tokio::test(flavor = "multi_thread")]
async fn a_changed_identity_key_blocks_the_add_until_accepted() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let bob = device::member_of_group(&admin, "bob").await;
    bob.wait_epoch(1).await;

    // The admin resets Bob: removes him and invites bob@chat again. Bob's new
    // phone signs up with a new identity key.
    admin
        .client
        .remove_user(&user("bob"))
        .await
        .check("remove bob");
    drop(bob);
    let mut new_bob = device::invited(&admin, "bob").await;

    let refused = admin.client.add_user(&user("bob")).await;
    assert!(
        matches!(&refused, Err(ClientError::IdentityKeyChanged(u)) if *u == user("bob")),
        "{refused:?}"
    );
    admin
        .wait_event(
            "the warning",
            |e| matches!(e, Event::IdentityKeyChanged { user: u } if *u == user("bob")),
        )
        .await;
    assert_eq!(admin.members(), vec![member(&admin)]);

    // The admin accepts the new key; now Bob's new phone can be added.
    assert!(
        admin
            .client
            .accept_identity_key(&user("bob"))
            .check("accept")
    );
    assert!(
        !admin
            .client
            .accept_identity_key(&user("bob"))
            .check("again")
    );
    admin
        .client
        .add_user(&user("bob"))
        .await
        .check("add new bob");
    new_bob
        .wait_event("joined", |e| matches!(e, Event::JoinedGroup))
        .await;
    admin
        .wait_members(&[member(&admin), member(&new_bob)])
        .await;
    let _ = Device::new;
}

/// The one-time KeyPackages the server holds for `device`.
async fn one_time(server: &TestServer, device: &Device) -> i64 {
    let id = device.device_id();
    server
        .query(move |db| {
            db.query_row(
                "SELECT count(*) FROM key_packages WHERE device_id = ?1 AND last_resort = 0",
                [id.as_slice()],
                |row| row.get(0),
            )
        })
        .await
}

// spec: key-directory/identity-key-lookup
// spec: key-directory/clients-verify-the-identity-chain
#[tokio::test(flavor = "multi_thread")]
async fn members_learn_identity_keys_without_using_keypackages() {
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let alice = device::member_of_group(&admin, "alice").await;
    let bob = device::member_of_group(&admin, "bob").await;
    let carol = device::member_of_group(&admin, "carol").await;
    let everyone = [member(&admin), member(&alice), member(&bob), member(&carol)];
    for device in [&admin, &alice, &bob, &carol] {
        device.wait_members(&everyone).await;
    }

    // Carol met Alice, Bob and the admin in her Welcome, Alice met Bob and
    // Carol in the admin's commits: each looked the keys up. Only the
    // admin's fetch for adding each of them used a KeyPackage.
    let target =
        i64::try_from(protocol::key_directory::ONE_TIME_KEY_PACKAGES_TARGET).check("target");
    for device in [&alice, &bob, &carol] {
        assert_eq!(one_time(&server, device).await, target - 1);
    }
    assert_eq!(
        one_time(&server, &admin).await,
        target,
        "nobody fetched the admin's"
    );
}

// spec: key-directory/clients-verify-the-identity-chain
// spec: group-membership/device-level-membership
#[tokio::test(flavor = "multi_thread")]
async fn members_follow_the_admins_re_add_of_a_reset_user() {
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let mut alice = device::member_of_group(&admin, "alice").await;
    let bob = device::member_of_group(&admin, "bob").await;
    alice
        .wait_members(&[member(&admin), member(&alice), member(&bob)])
        .await;

    // The admin resets Bob; Bob's new phone has a new identity key.
    admin
        .client
        .remove_user(&user("bob"))
        .await
        .check("remove bob");
    drop(bob);
    let mut new_bob = device::invited(&admin, "bob").await;
    let refused = admin.client.add_user(&user("bob")).await;
    assert!(
        matches!(&refused, Err(ClientError::IdentityKeyChanged(_))),
        "{refused:?}"
    );
    assert!(
        admin
            .client
            .accept_identity_key(&user("bob"))
            .check("accept")
    );
    admin
        .client
        .add_user(&user("bob"))
        .await
        .check("add new bob");
    new_bob
        .wait_event("joined", |e| matches!(e, Event::JoinedGroup))
        .await;

    // Alice still had Bob's old key pinned: she looks it up, accepts the
    // admin's commit, re-pins and is warned; she does not stall.
    alice
        .wait_event(
            "the key-changed warning",
            |e| matches!(e, Event::IdentityKeyChanged { user: u } if *u == user("bob")),
        )
        .await;
    let everyone = [member(&admin), member(&alice), member(&new_bob)];
    alice.wait_members(&everyone).await;
    assert!(
        !alice
            .client
            .accept_identity_key(&user("bob"))
            .check("nothing pending"),
        "the new key is already pinned"
    );
    assert!(
        !alice
            .sees_within_quiet(|e| matches!(e, Event::CommitRefused { .. }))
            .await
    );

    // The group goes on: the next admin commit reaches all three.
    let carol = device::member_of_group(&admin, "carol").await;
    let everyone = [
        member(&admin),
        member(&alice),
        member(&new_bob),
        member(&carol),
    ];
    alice.wait_members(&everyone).await;
    new_bob.wait_members(&everyone).await;
}
