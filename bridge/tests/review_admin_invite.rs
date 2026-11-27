//! review-admin-invite: end to end through the bridge. On the store reviewers'
//! server one reusable admin invite lets reviewer after reviewer join as the
//! admin; each join takes over the seat, the reviewer before is removed, and
//! the members rejoin the new admin's lobby and re-trust its key.

mod support;

use bridge::api::types::CoreEvent;
use support::Check;
use support::device::{self, Device, user};
use support::server::{ADMIN, TestServer};

// spec: device-identity/reusable-review-admin-invite
#[tokio::test(flavor = "multi_thread")]
async fn reviewers_take_turns_on_one_reusable_invite() {
    let server = TestServer::start_review().await;

    // Alice joins with the reusable invite, creates the lobby and invites Carol.
    let mut alice = device::admin_with_group(&server).await;
    let mut carol = device::member_of_group(&alice, "carol").await;

    // Bob joins with the very same invite text.
    let mut bob = Device::new().await;
    let id = bob
        .core
        .register(device::bootstrap_text(&server))
        .await
        .check("Bob registers with the same invite");
    assert_eq!(id, user(ADMIN));
    bob.core.start().await.check("start Bob");
    bob.wait_connected().await;

    // Alice is pushed out.
    alice
        .wait_event("Alice is removed", |e| matches!(e, CoreEvent::Removed))
        .await;

    // Bob creates a fresh lobby; Carol is re-added on her existing key and
    // must re-trust the admin's new safety number.
    bob.core.create_group().await.check("Bob creates the lobby");
    carol
        .wait_event(
            "the admin's safety number changed",
            |e| matches!(e, CoreEvent::IdentityKeyChanged { user: u } if *u == user(ADMIN)),
        )
        .await;
    assert_eq!(
        carol.core.identity_retrust_pending().check("pending"),
        vec![user(ADMIN)],
    );

    // And a third join works too: the invite is never used up.
    let mut again = Device::new().await;
    again
        .core
        .register(device::bootstrap_text(&server))
        .await
        .check("a third join");
    again.core.start().await.check("start the third");
    again.wait_connected().await;
    bob.wait_event("Bob is removed", |e| matches!(e, CoreEvent::Removed))
        .await;
}
