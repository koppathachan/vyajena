//! group-membership: inactivity.

use std::time::Duration;

use crate::DAY;

/// A member device that has not connected for this long is inactive
/// (Inactive devices are kept out).
pub const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(7 * DAY);
