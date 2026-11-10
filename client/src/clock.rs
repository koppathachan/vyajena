//! An injectable clock (client-core design D7). Rotation, token renewal,
//! inactivity and message times read the time from a [`Clock`], so tests can
//! move time forward by a week without waiting.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A source of the current wall-clock time.
pub trait Clock: Send + Sync + 'static {
    /// The current time.
    fn now(&self) -> SystemTime;
}

/// The real system clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// A clock that only moves when told to; for tests.
#[derive(Debug, Clone)]
pub struct ManualClock {
    now: Arc<Mutex<SystemTime>>,
}

impl ManualClock {
    /// A clock stopped at `start`.
    pub fn new(start: SystemTime) -> Self {
        Self {
            now: Arc::new(Mutex::new(start)),
        }
    }

    /// Moves the clock forward by `by`.
    pub fn advance(&self, by: Duration) {
        let mut now = self.now.lock().unwrap_or_else(PoisonError::into_inner);
        *now += by;
    }

    /// Sets the clock to `to`.
    pub fn set(&self, to: SystemTime) {
        *self.now.lock().unwrap_or_else(PoisonError::into_inner) = to;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> SystemTime {
        *self.now.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Milliseconds since the Unix epoch, as stored in the database. Times before
/// the epoch are negative.
pub fn unix_millis(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_millis()).unwrap_or(i64::MAX),
        Err(before) => i64::try_from(before.duration().as_millis())
            .map(|ms| -ms)
            .unwrap_or(i64::MIN),
    }
}

/// The inverse of [`unix_millis`].
pub fn from_unix_millis(ms: i64) -> SystemTime {
    let magnitude = Duration::from_millis(ms.unsigned_abs());
    if ms >= 0 {
        UNIX_EPOCH + magnitude
    } else {
        UNIX_EPOCH - magnitude
    }
}
