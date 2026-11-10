//! key-directory: the MLS ciphersuite, KeyPackage counts and the last-resort
//! KeyPackage refresh age.

use std::time::Duration;

use crate::DAY;

/// The only MLS ciphersuite the Pi accepts (KeyPackage upload).
pub const CIPHERSUITE_NAME: &str = "MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519";

/// RFC 9420 code point of [`CIPHERSUITE_NAME`].
pub const CIPHERSUITE_ID: u16 = 0x0001;

/// One-time KeyPackages a device keeps on the Pi (KeyPackage upload).
pub const ONE_TIME_KEY_PACKAGES_TARGET: usize = 20;

/// The Pi sends `LowKeyPackages` when a device's one-time count drops below
/// this (Replenishment signal).
pub const KEY_PACKAGES_LOW_WATER_MARK: usize = 5;

/// Last-resort KeyPackages per upload: exactly one (KeyPackage upload).
pub const LAST_RESORT_KEY_PACKAGES_PER_UPLOAD: usize = 1;

/// A device replaces its last-resort KeyPackage on the Pi once the current one
/// is this old (Last-resort KeyPackage refresh; open decision #4). Same age as
/// `group_membership::OWN_LEAF_MAX_AGE`.
pub const LAST_RESORT_MAX_AGE: Duration = Duration::from_secs(7 * DAY);

/// Whether a last-resort KeyPackage uploaded `age` ago is due for replacement
/// (Last-resort KeyPackage refresh): at [`LAST_RESORT_MAX_AGE`] or older.
pub fn last_resort_refresh_due(age: Duration) -> bool {
    age >= LAST_RESORT_MAX_AGE
}
