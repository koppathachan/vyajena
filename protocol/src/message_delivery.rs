//! message-delivery: message ids, queues, retention, wake pings.

use std::time::Duration;

use crate::DAY;

/// Length of a client-generated `Envelope.msg_id` (Envelope).
pub const MSG_ID_LEN: usize = 16;

/// Maximum application entries in one device's queue (Per-device queues).
pub const QUEUE_CAP: usize = 1_000;

/// `APPLICATION` queue entries are deleted this long after `accepted_at`
/// (Expiry and gap notice).
pub const QUEUE_ENTRY_TTL: Duration = Duration::from_secs(7 * DAY);

/// The server deduplicates accepted envelopes on (sender device, `msg_id`) for at
/// least this long (Idempotent sends).
pub const DEDUP_MIN_RETENTION: Duration = Duration::from_secs(7 * DAY);

/// The server sends at most one wake ping per device per this interval
/// (Content-free wake pings).
pub const WAKE_PING_MIN_INTERVAL: Duration = Duration::from_secs(60);

/// The most catch-up wake pings the sweep sends for one offline device's backlog
/// before it gives up, so a device whose app is gone is not pinged forever
/// (Sweep catch-up wake ping). The count is reset when the device drains its
/// backlog or reconnects. A platform "unregistered" response clears the token
/// before this bound is reached anyway (server-runtime: Push errors).
pub const CATCH_UP_MAX: u32 = 5;
