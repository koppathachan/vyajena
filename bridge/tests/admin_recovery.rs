//! admin-recovery: Admin re-key forces peer re-trust, end to end through the
//! bridge. A re-keyed peer (a reset user, or the recovered admin — the client
//! path is identical) surfaces as pending re-trust and is never silently
//! accepted; an explicit acknowledge flips it back to trusted.

mod support;

use bridge::api::types::CoreEvent;
use support::Check;
use support::device::{self, Device, user};
use support::server::TestServer;

// spec: admin-recovery/admin-re-key-forces-peer-re-trust
#[tokio::test(flavor = "multi_thread")]
async fn a_re_keyed_peer_surfaces_as_pending_re_trust_until_acknowledged() {
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let mut alice = device::member_of_group(&admin, "alice").await;
    let bob = device::member_of_group(&admin, "bob").await;
    alice
        .wait_until("alice sees bob", |c| {
            c.members()
                .is_ok_and(|m| m.iter().any(|d| d.user == user("bob")))
        })
        .await;
    assert!(
        alice
            .core
            .identity_retrust_pending()
            .check("none pending")
            .is_empty()
    );

    // The admin resets Bob; Bob's replacement device re-keys.
    admin
        .core
        .remove_user(user("bob"))
        .await
        .check("remove bob");
    drop(bob);
    let invite = admin
        .core
        .create_invite("bob".to_owned())
        .await
        .check("re-invite bob");
    let mut new_bob = Device::new().await;
    new_bob
        .core
        .register(invite.text.check("invite text"))
        .await
        .check("register new bob");
    new_bob.core.start().await.check("start new bob");
    new_bob.wait_connected().await;

    // The admin must accept the changed key before re-adding.
    let _ = admin.core.add_user(user("bob")).await;
    assert!(
        admin
            .core
            .accept_identity_key(user("bob"))
            .check("admin accepts")
    );
    admin.core.add_user(user("bob")).await.check("add new bob");
    new_bob
        .wait_event("joined", |e| matches!(e, CoreEvent::JoinedGroup))
        .await;

    // Alice sees the safety-number change and must re-trust: Bob is pinned (she
    // did not stall) but listed as pending re-trust, never silently accepted.
    alice
        .wait_event(
            "safety number changed",
            |e| matches!(e, CoreEvent::IdentityKeyChanged { user: u } if *u == user("bob")),
        )
        .await;
    assert_eq!(
        alice.core.identity_retrust_pending().check("pending"),
        vec![user("bob")],
    );

    // Acknowledging flips Bob back to trusted.
    assert!(
        alice
            .core
            .acknowledge_identity_key(user("bob"))
            .check("acknowledge")
    );
    assert!(
        alice
            .core
            .identity_retrust_pending()
            .check("cleared")
            .is_empty()
    );
}
