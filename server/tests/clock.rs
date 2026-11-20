//! server-core design D7: the injectable clock. Supports the time-based
//! requirements (server-runtime: Running locally runs on the system clock; tests
//! move a manual one).

mod support;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use server::runtime::clock::{from_unix_millis, unix_millis};
use server::runtime::{Clock, ManualClock, SystemClock};

// spec: server-runtime/running-locally
#[test]
fn manual_clock_moves_only_when_told() {
    let clock = support::manual_clock();
    let start = clock.now();
    assert_eq!(clock.now(), start);

    clock.advance(Duration::from_secs(7 * 24 * 60 * 60));
    assert_eq!(clock.now(), start + Duration::from_secs(604_800));

    // Clones share the same time, so a test can hold one and give one away.
    let shared = clock.clone();
    shared.advance(Duration::from_secs(1));
    assert_eq!(clock.now(), start + Duration::from_secs(604_801));

    clock.set(UNIX_EPOCH);
    assert_eq!(shared.now(), UNIX_EPOCH);
}

// spec: server-runtime/running-locally
#[test]
fn system_clock_reads_the_wall_clock() {
    let before = SystemTime::now();
    let now = SystemClock.now();
    assert!(now >= before);
    assert!(now <= SystemTime::now());
}

// spec: server-runtime/running-locally
#[test]
fn database_times_round_trip_in_milliseconds() {
    let t = support::test_epoch() + Duration::from_millis(1_234);
    assert_eq!(unix_millis(t), 1_794_182_401_234);
    assert_eq!(from_unix_millis(unix_millis(t)), t);
    let before_epoch = UNIX_EPOCH - Duration::from_millis(5);
    assert_eq!(unix_millis(before_epoch), -5);
    assert_eq!(from_unix_millis(-5), before_epoch);
    let _: &dyn Clock = &ManualClock::new(t);
}
