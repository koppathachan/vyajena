//! group-membership: Re-add after an unusable Welcome, the core's side, end
//! to end against the real server (welcome-recovery task 4.2). Bob's phone loses
//! the private keys of the KeyPackages it uploaded (what the iPhone in
//! design D1 may have hit), refuses the Welcome that adds it, asks the server to
//! be added again, and the admin's device re-adds it with no one tapping
//! anything.

mod support;

use client::{Event, ReaddOutcome, SendStatus};
use support::Check;
use support::device::{self, member};
use support::server::TestServer;

fn joined(e: &Event) -> bool {
    matches!(e, Event::JoinedGroup)
}

fn asked(outcome: ReaddOutcome) -> impl Fn(&Event) -> bool {
    move |e| matches!(e, Event::ReaddRequested { outcome: o } if *o == outcome)
}

/// Deletes every OpenMLS storage row but the storage-key check from the
/// device's database: its KeyPackage private keys are gone.
fn lose_key_package_keys(dir: &std::path::Path) {
    let db = rusqlite::Connection::open(dir.join(client::DATABASE_FILE)).check("open database");
    let deleted = db
        .execute(
            "DELETE FROM mls_storage WHERE slot != CAST('key-check' AS BLOB)",
            [],
        )
        .check("delete MLS rows");
    assert!(deleted > 0, "the device had KeyPackage keys to lose");
}

// spec: group-membership/re-add-after-an-unusable-welcome
#[tokio::test(flavor = "multi_thread")]
async fn bobs_phone_refuses_its_welcome_and_is_re_added() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let mut bob = device::invited_while_admin_away(&admin, "bob").await;

    // At start, with no group, Bob's core asks; the server has not added him
    // yet, so nothing changes and he waits for his first add.
    bob.wait_event(
        "the start ask: not a member yet",
        asked(ReaddOutcome::NotAMember),
    )
    .await;
    bob.client.stop();
    lose_key_package_keys(bob.data_dir());

    // The admin's phone comes back and adds Bob's phone; it cannot use the
    // Welcome.
    admin.client.start().await.check("start admin");
    bob.client.start().await.check("start bob");
    let refused = bob
        .wait_event("the Welcome is refused", |e| {
            matches!(e, Event::CommitRefused { .. })
        })
        .await;
    let Event::CommitRefused { reason } = refused else {
        unreachable!()
    };
    assert!(
        reason.starts_with("a Welcome could not be used"),
        "{reason}"
    );

    // It asks, and the admin's phone re-adds it from a fresh KeyPackage.
    bob.wait_event("the refusal ask", asked(ReaddOutcome::Asked))
        .await;
    bob.wait_event("bob joined", joined).await;
    admin.wait_members(&[member(&admin), member(&bob)]).await;
    bob.wait_members(&[member(&admin), member(&bob)]).await;
    // No "removed for inactivity" for a device that was never in.
    assert!(
        !bob.events()
            .iter()
            .any(|e| matches!(e, Event::RemovedForInactivity { .. }))
    );

    // And they chat.
    let id = admin.client.send_text("welcome, bob").check("send");
    admin
        .wait_event(
            "accepted",
            |e| matches!(e, Event::SendStatus { id: i, status: SendStatus::Accepted } if *i == id),
        )
        .await;
    bob.wait_event("the message arrives", |e| matches!(e, Event::Message(_)))
        .await;
}

// spec: group-membership/re-add-after-an-unusable-welcome
#[tokio::test(flavor = "multi_thread")]
async fn a_member_asks_nothing_at_start() {
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    bob.restart().await;
    bob.wait_connected().await;
    assert!(
        !bob.sees_within_quiet(|e| matches!(e, Event::ReaddRequested { .. }))
            .await
    );
}

// spec: device-identity/fresh-keys-for-a-new-signup
#[tokio::test(flavor = "multi_thread")]
async fn bob_reinstalls_after_the_admin_removed_him_and_joins_again() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let bob = device::member_of_group(&admin, "bob").await;
    let old_id = bob.device_id();
    admin
        .client
        .remove_user(&support::device::user("bob"))
        .await
        .check("remove bob");
    bob.client.stop();

    // Bob deletes and reinstalls the app; the Keychain kept his old keys.
    let invite = admin
        .client
        .create_invite("bob")
        .await
        .check("create invite");
    let mut new_bob = bob.reinstalled().await;
    new_bob
        .client
        .register(&invite.payload)
        .await
        .check("register after a reinstall");
    assert_ne!(new_bob.device_id(), old_id, "a new device id");
    new_bob.client.start().await.check("start");
    // The admin's phone sees Bob's new identity key and adds him once the
    // admin accepts it (key-directory: Clients verify the identity chain).
    admin
        .wait_event(
            "the key-changed warning",
            |e| matches!(e, Event::IdentityKeyChanged { user: u } if *u == support::device::user("bob")),
        )
        .await;
    assert!(
        admin
            .client
            .accept_identity_key(&support::device::user("bob"))
            .check("accept")
    );
    new_bob.wait_event("new bob joined", joined).await;
    admin
        .wait_members(&[member(&admin), member(&new_bob)])
        .await;
}

// spec: device-identity/fresh-keys-for-a-new-signup
#[tokio::test(flavor = "multi_thread")]
async fn linking_after_a_reinstall_starts_with_new_keys_and_no_identity_key() {
    use client::keystore::{KeyName, KeyStore as _};
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let bob = device::member_of_group(&admin, "bob").await;
    let old_auth = bob
        .keys
        .get(KeyName::DeviceAuthKey)
        .check("get")
        .check("auth key");
    assert!(bob.keys.get(KeyName::IdentityKey).check("get").is_some());
    bob.client.stop();

    let phone = bob.reinstalled().await;
    phone.client.link_request().await.check("link request");
    assert!(phone.keys.get(KeyName::IdentityKey).check("get").is_none());
    let new_auth = phone
        .keys
        .get(KeyName::DeviceAuthKey)
        .check("get")
        .check("auth key");
    assert_ne!(new_auth.as_slice(), old_auth.as_slice());
}

// spec: client-runtime/local-message-store
#[tokio::test(flavor = "multi_thread")]
async fn a_welcome_delivered_again_after_a_crash_is_not_refused() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    // Bob's phone joined from its Welcome; the network drops and the phone
    // restarts before its ACK reaches the server, which delivers the Welcome
    // again. Bob is already in: no refusal, no re-add.
    server.relay.cut();
    bob.restart().await;
    let id = admin.client.send_text("still there?").check("send");
    server.relay.restore();
    admin
        .wait_event(
            "accepted",
            |e| matches!(e, Event::SendStatus { id: i, status: SendStatus::Accepted } if *i == id),
        )
        .await;
    bob.wait_event("the message arrives", |e| matches!(e, Event::Message(_)))
        .await;
    let events = bob.events();
    assert!(
        !events.iter().any(|e| matches!(
            e,
            Event::CommitRefused { .. } | Event::ReaddRequested { .. } | Event::Rejoined
        )),
        "{events:?}"
    );
    admin.wait_members(&[member(&admin), member(&bob)]).await;
}
