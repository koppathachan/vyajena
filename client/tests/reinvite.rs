//! reinvite-user, end to end against the real server: an admin re-invites a
//! *present* member (no prior `Admin.RemoveUser`), the member's reinstalled
//! phone re-keys on register, and the admin's re-add evicts the stale old leaf
//! in the same `COMMIT` while peers re-trust the changed identity key
//! (reinvite-user: Re-key evicts the user's old group leaves; Re-invite changes
//! the user's identity key for peers; design D4/D5).

mod support;

use client::{ClientError, Event};
use support::Check;
use support::device::{self, member, user};
use support::server::TestServer;

// A re-invite of a present member re-keys them under the *same* user id with a
// fresh identity key (no prior remove), and the admin's re-add puts the new
// device in and drops the old revoked leaf in one COMMIT — so the lobby never
// holds both leaves and no orphan lingers.
//
// spec: reinvite-user/re-key-evicts-the-user-s-old-group-leaves-and-rejoins-through-the-normal-path
// spec: device-identity/re-inviting-an-existing-user
#[tokio::test(flavor = "multi_thread")]
async fn old_leaf_is_evicted_and_the_new_device_joins() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let old_bob = device::member_of_group(&admin, "bob").await;
    admin
        .wait_members(&[member(&admin), member(&old_bob)])
        .await;
    let old_member = member(&old_bob);
    drop(old_bob);

    // The admin re-invites the present `bob@chat` — no RemoveUser. Bob's
    // reinstalled phone registers with a fresh identity key and the same user
    // id; the server re-keys and revokes the old device.
    let mut new_bob = device::invited(&admin, "bob").await;
    assert_eq!(new_bob.user(), user("bob"), "same user id");
    assert_ne!(new_bob.device_id(), old_member.device, "a fresh device");

    // The changed key blocks the add until the admin accepts it (design D4).
    let refused = admin.client.add_user(&user("bob")).await;
    assert!(
        matches!(&refused, Err(ClientError::IdentityKeyChanged(u)) if *u == user("bob")),
        "{refused:?}"
    );
    admin
        .wait_event(
            "the safety-number warning",
            |e| matches!(e, Event::IdentityKeyChanged { user: u } if *u == user("bob")),
        )
        .await;
    assert!(
        admin
            .client
            .accept_identity_key(&user("bob"))
            .check("accept")
    );

    // The admin re-adds: one COMMIT adds Bob's new phone and evicts his stale
    // old leaf. The lobby ends with exactly the admin and Bob's new leaf —
    // without the eviction the old leaf would still sit here as a third member.
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
    assert!(
        !admin.members().contains(&old_member),
        "the old revoked leaf is evicted"
    );
}

// A lobby member offline during the re-key catches up on reconnect: it
// receives the one commit that removed Bob's old leaf and added his new one,
// and ends with only Bob's new leaf — no orphan to reconcile.
//
// spec: reinvite-user/re-key-evicts-the-user-s-old-group-leaves-and-rejoins-through-the-normal-path
#[tokio::test(flavor = "multi_thread")]
async fn member_offline_during_the_re_key_catches_up() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let mut alice = device::member_of_group(&admin, "alice").await;
    let old_bob = device::member_of_group(&admin, "bob").await;
    alice
        .wait_members(&[member(&admin), member(&alice), member(&old_bob)])
        .await;
    let old_member = member(&old_bob);
    drop(old_bob);

    // Alice goes offline, then the admin re-invites and re-adds the re-keyed
    // Bob while she is away.
    alice.client.stop();
    let mut new_bob = device::invited(&admin, "bob").await;
    let _ = admin.client.add_user(&user("bob")).await;
    admin
        .wait_event(
            "the safety-number warning",
            |e| matches!(e, Event::IdentityKeyChanged { user: u } if *u == user("bob")),
        )
        .await;
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

    // Alice reconnects and catches up: she lands on exactly the admin, herself
    // and Bob's new leaf, with the old leaf gone.
    alice.restart().await;
    alice
        .wait_members(&[member(&admin), member(&alice), member(&new_bob)])
        .await;
    assert!(
        !alice.members().contains(&old_member),
        "Alice caught up past the old leaf"
    );
}

// A peer sees the changed identity key as a re-trust prompt after a re-invite
// and is left pending re-trust — nothing is silently accepted (design D4;
// delegated to key-directory: Clients verify the identity chain).
//
// spec: reinvite-user/re-invite-changes-the-user-s-identity-key-for-peers
#[tokio::test(flavor = "multi_thread")]
async fn members_see_a_re_trust_prompt_after_a_re_invite() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let mut alice = device::member_of_group(&admin, "alice").await;
    let old_bob = device::member_of_group(&admin, "bob").await;
    alice
        .wait_members(&[member(&admin), member(&alice), member(&old_bob)])
        .await;
    assert!(
        alice
            .client
            .identity_retrust_pending()
            .check("none pending")
            .is_empty()
    );
    drop(old_bob);

    // The admin re-invites present bob (no remove); his new phone re-keys and
    // the admin re-adds it after accepting the new key.
    let mut new_bob = device::invited(&admin, "bob").await;
    let _ = admin.client.add_user(&user("bob")).await;
    admin
        .wait_event(
            "admin warning",
            |e| matches!(e, Event::IdentityKeyChanged { user: u } if *u == user("bob")),
        )
        .await;
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

    // Alice pins Bob's new key to keep decrypting, is warned, and is left
    // pending re-trust — never silently trusted.
    alice
        .wait_event(
            "the safety-number-changed warning",
            |e| matches!(e, Event::IdentityKeyChanged { user: u } if *u == user("bob")),
        )
        .await;
    alice
        .wait_members(&[member(&admin), member(&alice), member(&new_bob)])
        .await;
    assert_eq!(
        alice.client.identity_retrust_pending().check("pending"),
        vec![user("bob")],
        "the re-keyed peer is unverified / pending re-trust"
    );
}
