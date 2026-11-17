//! group-membership: Re-add after an unusable Welcome (welcome-recovery
//! design D2–D4). The checks a `Keys.RequestReadd` must pass, and the
//! per-device limit on how often a device may ask.
//!
//! The limit is kept in memory: it only stops a commit loop, so a Pi restart
//! that forgets it is harmless, and no schema change blocks a rollback.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::SystemTime;

use protocol::device_identity::DeviceId;
use protocol::group_membership::{MAX_READD_REQUESTS_PER_DAY, READD_REQUEST_WINDOW};
use rusqlite::{Connection, params};

use crate::store::StoreError;

fn i64_of(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// Whether `device` is a non-revoked device with a leaf in the main group's
/// tree.
pub fn is_member(conn: &Connection, device: &DeviceId) -> Result<bool, StoreError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM mls_members m JOIN devices d ON d.device_id = m.device_id
                        JOIN mls_groups g ON g.group_id = m.group_id
                        WHERE m.device_id = ?1 AND d.revoked_at IS NULL AND g.kind = 'main')",
        params![device.as_slice()],
        |row| row.get(0),
    )?)
}

/// Whether a Welcome for `device` with a `server_seq` above `after` is still
/// queued (not yet acked).
pub fn welcome_queued(
    conn: &Connection,
    device: &DeviceId,
    after: u64,
) -> Result<bool, StoreError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM deliveries
                        WHERE device_id = ?1 AND welcome = 1 AND server_seq > ?2)",
        params![device.as_slice(), i64_of(after)],
        |row| row.get(0),
    )?)
}

/// When each device asked, within the last [`READD_REQUEST_WINDOW`].
#[derive(Debug, Default)]
pub struct ReaddAsks {
    asks: Mutex<HashMap<DeviceId, Vec<SystemTime>>>,
}

impl ReaddAsks {
    fn recent(times: &mut Vec<SystemTime>, now: SystemTime) {
        times.retain(|at| {
            now.duration_since(*at)
                .map_or(true, |age| age < READD_REQUEST_WINDOW)
        });
    }

    /// Whether `device` may ask at `now`: fewer than
    /// [`MAX_READD_REQUESTS_PER_DAY`] asks in the window before it.
    pub fn allowed(&self, device: &DeviceId, now: SystemTime) -> bool {
        let mut asks = self.asks.lock().unwrap_or_else(|e| e.into_inner());
        let times = asks.entry(*device).or_default();
        Self::recent(times, now);
        times.len() < MAX_READD_REQUESTS_PER_DAY
    }

    /// Records an accepted ask of `device` at `now`.
    pub fn record(&self, device: &DeviceId, now: SystemTime) {
        let mut asks = self.asks.lock().unwrap_or_else(|e| e.into_inner());
        let times = asks.entry(*device).or_default();
        Self::recent(times, now);
        times.push(now);
    }
}
