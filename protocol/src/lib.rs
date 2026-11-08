//! Rules the Pi and the device core must agree on that the `vyajena.v1` protos
//! cannot express: limits, counts, timings and signing labels. Later it also
//! holds logic both sides run (device certificates, device ids, the commit
//! authority check).
//!
//! One module per spec capability in `openspec/specs/`. Each item names the
//! requirement it comes from. No generated code lives here.

#![forbid(unsafe_code)]

pub mod device_identity;
pub mod group_membership;
pub mod key_directory;
pub mod message_delivery;
pub mod wire_transport;

/// Seconds in a day, for the day-based durations.
pub(crate) const DAY: u64 = 24 * 60 * 60;
