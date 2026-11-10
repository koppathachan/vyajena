//! group-membership on the device, end to end against the real Pi
//! (client-core tasks 5.1, 5.2; design D5, D8).

mod support;

use client::Event;
use support::Check;
use support::device::{self, Device, member, user};
use support::pi::{ADMIN, TestPi};

// spec: group-membership/group-creation
// spec: device-identity/admin-awareness
// spec: device-identity/bootstrap-admin
#[tokio::test(flavor = "multi_thread")]
async fn the_admin_creates_the_group_with_the_pi_as_external_sender() {
    let pi = TestPi::start().await;
    let admin = device::admin(&pi).await;
    assert!(admin.client.is_admin().check("is admin"));
    admin.client.create_group().await.check("create group");

    assert_eq!(admin.epoch(), Some(0));
    assert_eq!(admin.members(), vec![member(&admin)]);
    // The Pi accepted the public state: it checks the Pi external sender and
    // the 0xF0A1 extension naming the admin.
    let (epoch, members): (i64, i64) = pi
        .query(|conn| {
            conn.query_row(
                "SELECT (SELECT epoch FROM mls_group), (SELECT count(*) FROM mls_members)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
        })
        .await;
    assert_eq!((epoch, members), (0, 1));

    // A second group is refused.
    let again = admin.client.create_group().await;
    assert!(again.is_err(), "a second group: {again:?}");
    let _ = ADMIN;
}

// spec: group-membership/commit-authority
// spec: group-membership/device-level-membership
// spec: message-delivery/welcome-travels-with-its-commit
#[tokio::test(flavor = "multi_thread")]
async fn the_admin_adds_a_user_and_every_member_follows() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    let mut alice = device::member_of_group(&admin, "alice").await;
    assert!(!alice.client.is_admin().check("is admin"));
    alice.wait_epoch(1).await;

    let mut bob = device::invited(&admin, "bob").await;
    admin.client.add_user(&user("bob")).await.check("add bob");
    bob.wait_event("bob joined", |e| matches!(e, Event::JoinedGroup))
        .await;

    let everyone = [member(&admin), member(&alice), member(&bob)];
    for device in [&admin, &alice, &bob] {
        device.wait_epoch(2).await;
        device.wait_members(&everyone).await;
    }
    // Alice verified Bob's new leaf (and pinned his key) without refusing.
    assert!(
        !alice
            .sees_within_quiet(|e| matches!(e, Event::CommitRefused { .. }))
            .await
    );
    // The admin's core sent each Welcome inside the COMMIT that made it:
    // the Pi recorded every add commit with its Welcome bundle, and only
    // commits (no client envelope of another kind) carry one.
    let admin_id = admin.device_id();
    let (commits, with_welcome): (i64, i64) = pi
        .query(move |db| {
            db.query_row(
                "SELECT count(*), count(welcome_sha256) FROM dedup WHERE sender_device_id = ?1",
                [admin_id.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
        })
        .await;
    assert_eq!((commits, with_welcome), (2, 2), "two adds, two Welcomes");
}

// spec: group-membership/removing-a-user
#[tokio::test(flavor = "multi_thread")]
async fn the_admin_removes_a_user() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    let alice = device::member_of_group(&admin, "alice").await;
    let bob = device::member_of_group(&admin, "bob").await;
    alice.wait_epoch(2).await;
    let bob_device = bob.device_id();

    admin
        .client
        .remove_user(&user("bob"))
        .await
        .check("remove bob");
    let rest = [member(&admin), member(&alice)];
    admin.wait_members(&rest).await;
    alice.wait_members(&rest).await;

    // Admin.RemoveUser revoked Bob's device.
    let revoked: bool = pi
        .query(move |conn| {
            conn.query_row(
                "SELECT revoked_at IS NOT NULL FROM devices WHERE device_id = ?1",
                [bob_device.as_slice()],
                |row| row.get(0),
            )
        })
        .await;
    assert!(revoked);
    let _ = Device::new;
}

/// Whether the admin's database holds an own commit in flight.
fn commit_in_flight(device: &Device) -> bool {
    let path = device.data_dir().join(client::DATABASE_FILE);
    rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .and_then(|conn| {
            conn.query_row("SELECT count(*) FROM outgoing_commit", [], |row| {
                row.get::<_, i64>(0)
            })
        })
        .is_ok_and(|n| n > 0)
}

// spec: message-delivery/idempotent-sends
// spec: client-runtime/local-message-store
#[tokio::test(flavor = "multi_thread")]
async fn a_commit_in_flight_survives_a_crash_and_is_resent_identically() {
    let pi = TestPi::start().await;
    let mut admin = device::admin_with_group(&pi).await;
    let mut bob = device::invited(&admin, "bob").await;

    // The admin's device crashes while its commit adding Bob is in flight,
    // with the network gone, so the result is lost.
    let adding = {
        let user = user("bob");
        let client = &admin.client;
        async move { client.add_user(&user).await }
    };
    let watch = async {
        let deadline = tokio::time::Instant::now() + support::device::WAIT;
        while !commit_in_flight(&admin) {
            assert!(
                tokio::time::Instant::now() < deadline,
                "no commit in flight"
            );
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        pi.relay.cut();
    };
    let (result, ()) = tokio::join!(
        tokio::time::timeout(std::time::Duration::from_secs(2), adding),
        watch
    );
    drop(result);
    admin.restart().await;
    pi.relay.restore();

    // After the restart the stored commit is sent again with the same bytes:
    // the Pi applies it once, and Bob joins once.
    bob.wait_event("joined", |e| matches!(e, Event::JoinedGroup))
        .await;
    let both = [member(&admin), member(&bob)];
    admin.wait_members(&both).await;
    bob.wait_members(&both).await;
    admin.wait_epoch(1).await;
    assert!(!commit_in_flight(&admin));
    assert!(
        !bob.sees_within_quiet(|e| matches!(e, Event::JoinedGroup))
            .await
    );
    let commits: i64 = pi
        .query(|conn| conn.query_row("SELECT count(*) FROM dedup", [], |row| row.get(0)))
        .await;
    // One accepted envelope from the admin's device: the commit, once.
    assert_eq!(commits, 1);
}
