//! manual-presence in the device core (change `manual-presence`): the
//! latest-`updated_at`-wins reconciliation of others' flags, the own-flag dirty
//! bit, and the own flag reaching another user end to end against the real
//! server.

mod support;

use client::Presence;
use client::keystore::MemoryKeyStore;
use protocol::presence::Presence as WirePresence;
use support::Check;
use support::device::{self, Device, member};
use support::server::TestServer;

const ALICE: &str = "alice@chat";

/// The admin, Alice and Bob, all in the group and seeing each other.
async fn group(server: &TestServer) -> (Device, Device, Device) {
    let admin = device::admin_with_group(server).await;
    let alice = device::member_of_group(&admin, "alice").await;
    let bob = device::member_of_group(&admin, "bob").await;
    let everyone = [member(&admin), member(&alice), member(&bob)];
    for device in [&admin, &alice, &bob] {
        device.wait_members(&everyone).await;
    }
    (admin, alice, bob)
}

// spec: presence/latest-value-wins
#[test]
fn a_presence_update_applies_only_when_it_is_newer() {
    let dir = support::temp_dir();
    let keys = MemoryKeyStore::new();
    let store = support::open_store(dir.path(), &keys);

    // An unknown peer has no flag (the UI shows no dot).
    assert_eq!(store.peer_presence(ALICE).check("unknown"), None);

    // The first value is applied.
    assert!(
        store
            .apply_peer_presence(ALICE, WirePresence::Online, 200)
            .check("set")
    );
    assert_eq!(
        store.peer_presence(ALICE).check("online"),
        Some(WirePresence::Online)
    );

    // An older `updated_at` is ignored (latest wins).
    assert!(
        !store
            .apply_peer_presence(ALICE, WirePresence::Offline, 100)
            .check("older")
    );
    assert_eq!(
        store.peer_presence(ALICE).check("still online"),
        Some(WirePresence::Online)
    );

    // An equal `updated_at` is not newer, so it is ignored too.
    assert!(
        !store
            .apply_peer_presence(ALICE, WirePresence::Offline, 200)
            .check("equal")
    );
    assert_eq!(
        store.peer_presence(ALICE).check("still online"),
        Some(WirePresence::Online)
    );

    // A newer `updated_at` supersedes.
    assert!(
        store
            .apply_peer_presence(ALICE, WirePresence::Offline, 300)
            .check("newer")
    );
    assert_eq!(
        store.peer_presence(ALICE).check("now offline"),
        Some(WirePresence::Offline)
    );
}

// spec: presence/default-status-before-first-set
// spec: presence/toggle-your-status-on-the-profile-screen
#[test]
fn the_own_flag_defaults_offline_and_carries_a_send_pending_bit() {
    let dir = support::temp_dir();
    let keys = MemoryKeyStore::new();
    let store = support::open_store(dir.path(), &keys);

    // Offline by default, with nothing pending to send.
    assert_eq!(store.pending_self_presence().check("pending"), None);

    // Setting Online marks it changed and pending.
    assert!(
        store
            .set_self_presence(WirePresence::Online)
            .check("set online")
    );
    assert_eq!(
        store.pending_self_presence().check("pending online"),
        Some(WirePresence::Online)
    );

    // Once sent, nothing is pending.
    store.clear_self_presence_dirty().check("clear");
    assert_eq!(store.pending_self_presence().check("not pending"), None);

    // Re-setting the same value does not count as a change but is still queued
    // to send (so an offline toggle is not lost).
    assert!(!store.set_self_presence(WirePresence::Online).check("same"));
    assert_eq!(
        store.pending_self_presence().check("pending again"),
        Some(WirePresence::Online)
    );
}

// spec: presence/setting-your-status
// spec: presence/broadcasting-a-status-change
#[tokio::test(flavor = "multi_thread")]
async fn alices_manual_flag_reaches_bob_and_round_trips_for_her() {
    let server = TestServer::start().await;
    let (_admin, alice, bob) = group(&server).await;

    // Offline is the default for both the setter and the viewer.
    assert_eq!(
        alice.client.my_presence().check("my default"),
        Presence::Offline
    );

    // Alice goes Online.
    alice
        .client
        .set_presence(Presence::Online)
        .check("set online");
    assert_eq!(alice.client.my_presence().check("mine"), Presence::Online);

    // Bob, who shares the lobby, learns it within a stream round-trip.
    bob.wait_until("Bob sees Alice online", |c| {
        c.presence_of(&device::user("alice")).unwrap_or(None) == Some(Presence::Online)
    })
    .await;

    // Alice goes Offline again and Bob follows (latest wins).
    alice
        .client
        .set_presence(Presence::Offline)
        .check("set offline");
    bob.wait_until("Bob sees Alice offline", |c| {
        c.presence_of(&device::user("alice")).unwrap_or(None) == Some(Presence::Offline)
    })
    .await;
}
