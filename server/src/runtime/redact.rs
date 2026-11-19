//! pi-runtime (relay-privacy-logging): a short stable hash of an id for logs.
//!
//! A `device_id` is a stable identifier. Logs show this hash instead of the raw
//! id, so lines for one device can still be correlated while a leaked journal
//! never hands out the id itself. The value is deterministic (same input → same
//! hash) but is not a prefix of the input (D1).

use std::fmt;

use sha2::{Digest, Sha256};

/// The first 12 hex chars (48 bits) of `SHA-256(bytes)`, for logging an id
/// without revealing it.
pub fn redact(bytes: &[u8]) -> Redacted {
    Redacted(Sha256::digest(bytes).into())
}

/// Displays as the first 12 hex chars of the hash of the bytes it was built
/// from. Returned by [`redact`]; use it as `%redact(&id)` in a `tracing` field.
pub struct Redacted([u8; 32]);

impl fmt::Display for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in &self.0[..6] {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}
