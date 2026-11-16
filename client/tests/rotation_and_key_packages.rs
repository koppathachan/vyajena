//! Own-leaf rotation and KeyPackages on the device, end to end against the
//! real Pi (client-core task 5.4; open decision #4, design D6).

mod support;

use std::time::Duration;

use client::Event;
use support::Check;
use support::device::{self, Device};
use support::pi::TestPi;

const WEEK: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const HOUR: Duration = Duration::from_secs(60 * 60);

/// Lets the device's timer see the clock (it looks every 100 ms).
async fn let_timers_run() {
    tokio::time::sleep(Duration::from_millis(400)).await;
}

// spec: group-membership/own-leaf-rotation
#[tokio::test(flavor = "multi_thread")]
async fn in_a_quiet_group_a_week_old_leaf_is_rotated() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    let alice = device::member_of_group(&admin, "alice").await;
    alice.wait_epoch(1).await;
    let first = pi.last_resort(&alice.device_id()).await;

    // Six days: nothing.
    alice.clock.advance(6 * 24 * HOUR);
    let_timers_run().await;
    assert_eq!(admin.epoch(), Some(1));

    // Seven days: due; it commits within the jitter (at most an hour) of the
    // timer check that found it due.
    alice.clock.advance(24 * HOUR);
    pi.wait_last_resort_replaced(&alice.device_id(), &first)
        .await;
    alice.clock.advance(HOUR);
    admin.wait_epoch(2).await;
    alice.wait_epoch(2).await;

    // The new leaf is fresh: no second rotation the same week.
    alice.clock.advance(2 * 24 * HOUR);
    let_timers_run().await;
    alice.clock.advance(HOUR);
    let_timers_run().await;
    assert_eq!(admin.epoch(), Some(2));
}

// spec: group-membership/own-leaf-rotation
// spec: message-delivery/strict-epoch-rule
#[tokio::test(flavor = "multi_thread")]
async fn two_devices_rotating_at_once_both_get_their_rotation_in() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    let alice = device::member_of_group(&admin, "alice").await;
    let bob = device::member_of_group(&admin, "bob").await;
    alice.wait_epoch(2).await;
    bob.wait_epoch(2).await;
    let alice_first = pi.last_resort(&alice.device_id()).await;
    let bob_first = pi.last_resort(&bob.device_id()).await;

    alice.clock.advance(WEEK);
    bob.clock.advance(WEEK);
    // Both timers found the rotation due.
    pi.wait_last_resort_replaced(&alice.device_id(), &alice_first)
        .await;
    pi.wait_last_resort_replaced(&bob.device_id(), &bob_first)
        .await;
    alice.clock.advance(HOUR);
    bob.clock.advance(HOUR);
    // Both rotate: whichever loses the race catches up and retries.
    for device in [&admin, &alice, &bob] {
        device.wait_epoch(4).await;
    }
    for device in [&admin, &alice, &bob] {
        assert!(
            !device
                .events()
                .iter()
                .any(|e| matches!(e, Event::CommitRefused { .. }))
        );
    }
}

async fn last_resort(pi: &TestPi, device: [u8; 16]) -> Vec<u8> {
    pi.query(move |conn| {
        conn.query_row(
            "SELECT key_package FROM key_packages WHERE device_id = ?1 AND last_resort = 1",
            [device.as_slice()],
            |row| row.get(0),
        )
    })
    .await
}

async fn one_time_count(pi: &TestPi, device: [u8; 16]) -> i64 {
    pi.query(move |conn| {
        conn.query_row(
            "SELECT count(*) FROM key_packages WHERE device_id = ?1 AND last_resort = 0",
            [device.as_slice()],
            |row| row.get(0),
        )
    })
    .await
}

// spec: key-directory/last-resort-keypackage-refresh
// spec: key-directory/keypackage-upload
#[tokio::test(flavor = "multi_thread")]
async fn a_week_old_last_resort_key_package_is_replaced() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    // Bob signs up while the admin's device is away, so nobody adds him yet.
    let bob = device::invited_while_admin_away(&admin, "bob").await;
    let id = bob.device_id();
    let first = last_resort(&pi, id).await;
    assert_eq!(one_time_count(&pi, id).await, 20);

    bob.clock.advance(6 * 24 * HOUR);
    let_timers_run().await;
    assert_eq!(last_resort(&pi, id).await, first);

    bob.clock.advance(24 * HOUR);
    let deadline = tokio::time::Instant::now() + support::device::WAIT;
    while last_resort(&pi, id).await == first {
        assert!(tokio::time::Instant::now() < deadline, "no refresh");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // One-time KeyPackages are untouched; the new last-resort one works:
    // once they run out the Pi hands it out and Bob can still be added.
    assert_eq!(one_time_count(&pi, id).await, 20);
    pi.query(move |conn| {
        conn.execute(
            "DELETE FROM key_packages WHERE device_id = ?1 AND last_resort = 0",
            [id.as_slice()],
        )
    })
    .await;
    let mut bob = bob;
    // The admin's device comes back and adds him (group-membership:
    // Automatic add of new users).
    admin.client.start().await.check("start admin");
    bob.wait_event("joined with the last-resort KeyPackage", |e| {
        matches!(e, Event::JoinedGroup)
    })
    .await;
}

// spec: key-directory/replenishment-signal
#[tokio::test(flavor = "multi_thread")]
async fn a_low_key_packages_notice_makes_the_device_upload_more() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    let mut bob = device::invited_while_admin_away(&admin, "bob").await;
    let id = bob.device_id();
    // Leave Bob five one-time KeyPackages; the admin's fetch takes one more.
    pi.query(move |conn| {
        conn.execute(
            "DELETE FROM key_packages WHERE id IN (
                 SELECT id FROM key_packages WHERE device_id = ?1 AND last_resort = 0
                 ORDER BY id LIMIT 15)",
            [id.as_slice()],
        )
    })
    .await;
    assert_eq!(one_time_count(&pi, id).await, 5);
    admin.client.start().await.check("start admin");
    bob.wait_event("joined", |e| matches!(e, Event::JoinedGroup))
        .await;
    let deadline = tokio::time::Instant::now() + support::device::WAIT;
    while one_time_count(&pi, id).await < 20 {
        assert!(tokio::time::Instant::now() < deadline, "no top-up");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(one_time_count(&pi, id).await, 20);
    let _ = Device::new;
}
