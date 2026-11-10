//! External tests for `protocol::key_directory` (public API only).

use std::time::Duration;

use protocol::key_directory::{LAST_RESORT_MAX_AGE, last_resort_refresh_due};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

// spec: key-directory/last-resort-keypackage-refresh
#[test]
fn last_resort_key_package_is_replaced_weekly() {
    assert_eq!(LAST_RESORT_MAX_AGE, 7 * DAY);
    // Same schedule as own-leaf rotation (decision #4).
    assert_eq!(
        LAST_RESORT_MAX_AGE,
        protocol::group_membership::OWN_LEAF_MAX_AGE
    );
    // Weekly refresh: Bob's 7-day-old last-resort KeyPackage is due.
    assert!(last_resort_refresh_due(7 * DAY));
    assert!(last_resort_refresh_due(8 * DAY));
    assert!(!last_resort_refresh_due(7 * DAY - Duration::from_secs(1)));
    assert!(!last_resort_refresh_due(Duration::ZERO));
}
