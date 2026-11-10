//! client-runtime: Connection lifecycle. Reconnect delays: exponential from
//! [`Backoff::FIRST`] to [`Backoff::MAX`] with random jitter, reset once a
//! connection is up.

use std::time::Duration;

/// Reconnect delays.
#[derive(Debug, Clone)]
pub struct Backoff {
    next: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}

impl Backoff {
    /// The first delay after a drop.
    pub const FIRST: Duration = Duration::from_millis(250);
    /// The longest delay.
    pub const MAX: Duration = Duration::from_secs(30);

    /// Starts at [`Backoff::FIRST`].
    pub fn new() -> Self {
        Self { next: Self::FIRST }
    }

    /// The delay before the next attempt: the current step with up to 50%
    /// random jitter added, then the step doubles up to [`Backoff::MAX`].
    pub fn next_delay(&mut self) -> Duration {
        let step = self.next;
        self.next = (self.next * 2).min(Self::MAX);
        let jitter_ms = u64::try_from(step.as_millis() / 2).unwrap_or(0);
        step + Duration::from_millis(random_below(jitter_ms.saturating_add(1)))
    }

    /// A connection is up: the next drop starts again at [`Backoff::FIRST`].
    pub fn reset(&mut self) {
        self.next = Self::FIRST;
    }
}

/// A random number in `[0, bound)` (0 when `bound` is 0 or randomness fails).
pub fn random_below(bound: u64) -> u64 {
    if bound == 0 {
        return 0;
    }
    let mut bytes = [0u8; 8];
    if getrandom::fill(&mut bytes).is_err() {
        return 0;
    }
    u64::from_le_bytes(bytes) % bound
}
