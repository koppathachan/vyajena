//! key-directory: the MLS ciphersuite and KeyPackage counts.

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
