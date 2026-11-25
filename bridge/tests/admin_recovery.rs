//! admin-recovery: Admin re-key forces peer re-trust, end to end through the
//! bridge. A re-keyed peer (a reset user, or the recovered admin — the client
//! path is identical) surfaces as pending re-trust and is never silently
//! accepted; an explicit acknowledge flips it back to trusted.

mod support;

use bridge::api::types::CoreEvent;
use support::Check;
use support::device::{self, Device, user};
use support::server::{ADMIN, TestServer};

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

// spec: admin-recovery/break-glass-re-seeds-the-admin-s-lobby
#[tokio::test(flavor = "multi_thread")]
async fn admin_re_key_re_seeds_the_lobby_and_the_member_rejoins() {
    let mut server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let mut alice = device::member_of_group(&admin, "alice").await;
    // Alice pinned the admin's original key when she joined; nothing is pending.
    assert!(
        alice
            .core
            .identity_retrust_pending()
            .check("none pending")
            .is_empty()
    );

    // Operator break-glass on the box: reset the admin and re-seed the lobby.
    let report = server::runtime::admin_reset::reset_admin(
        &server.state.store,
        &server.state.config,
        server.now(),
    )
    .await
    .check("admin reset");
    assert!(report.lobby_reseeded, "the lobby was re-seeded");
    assert_eq!(report.members_to_rejoin, 1, "alice rejoins");
    // The pre-reset admin device is revoked and its lobby is gone.
    drop(admin);

    // Restart re-mints the bootstrap invite; the recovered admin registers with
    // a fresh identity key and re-creates the lobby. CreateGroup succeeding is
    // the proof the old lobby was dropped — it would be ALREADY_EXISTS
    // otherwise.
    server.restart().await;
    let mut new_admin = Device::new().await;
    let id = new_admin
        .core
        .register(device::bootstrap_text(&server))
        .await
        .check("register the recovered admin");
    assert_eq!(id, user(ADMIN));
    new_admin
        .core
        .start()
        .await
        .check("start the recovered admin");
    new_admin.wait_connected().await;
    new_admin
        .core
        .create_group()
        .await
        .check("re-create the lobby");

    // No manual add and NO admin reconnect here: the recovered admin connected
    // before the lobby existed, so the connect-time waiting announce was missed.
    // `Admin.CreateGroup` must flush the waiting notices so the admin re-adds the
    // re-announced member on its own. Without that flush alice would sit waiting
    // forever and the `IdentityKeyChanged` wait below would time out.
    //
    // Alice sees the admin's safety number change and must re-trust: her client
    // pins the new key to keep decrypting, but marks the admin pending re-trust
    // and never silently accepts the change.
    alice
        .wait_event(
            "the admin's safety number changed",
            |e| matches!(e, CoreEvent::IdentityKeyChanged { user: u } if *u == user(ADMIN)),
        )
        .await;
    assert_eq!(
        alice.core.identity_retrust_pending().check("pending"),
        vec![user(ADMIN)],
        "the recovered admin is pending re-trust for the rejoined member",
    );

    // Acknowledging flips the admin back to trusted.
    assert!(
        alice
            .core
            .acknowledge_identity_key(user(ADMIN))
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
