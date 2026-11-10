//! key-directory: Clients verify the identity chain, end to end against the
//! real Pi: identity keys pinned on first sight, and a changed key blocks
//! adding that user's devices until the user accepts it (client-core task
//! 5.2, design D5).

mod support;

use client::{ClientError, Event};
use support::Check;
use support::device::{self, Device, member, user};
use support::pi::TestPi;

// spec: key-directory/clients-verify-the-identity-chain
// spec: group-membership/removing-a-user
#[tokio::test(flavor = "multi_thread")]
async fn a_changed_identity_key_blocks_the_add_until_accepted() {
    let pi = TestPi::start().await;
    let mut admin = device::admin_with_group(&pi).await;
    let bob = device::member_of_group(&admin, "bob").await;
    bob.wait_epoch(1).await;

    // The admin resets Bob: removes him and invites bob@pi again. Bob's new
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
