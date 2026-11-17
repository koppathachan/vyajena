//! Rules the Pi and the device core must agree on that the `vyajena.v1` protos
//! cannot express: limits, counts, timings and signing labels, plus the logic
//! both sides run (device certificate signatures, device ids, the commit
//! authority rule).
//!
//! One module per spec capability in `openspec/specs/`. Each item names the
//! requirement it comes from. No generated code and no OpenMLS types live here:
//! each side decodes protobuf and reads OpenMLS itself, then calls these
//! functions on plain types (pi-server-core design D3).

#![forbid(unsafe_code)]

pub mod device_identity;
pub mod group_membership;
pub mod key_directory;
pub mod message_content;
pub mod message_delivery;
pub mod wire_transport;

/// Seconds in a day, for the day-based durations.
pub(crate) const DAY: u64 = 24 * 60 * 60;
