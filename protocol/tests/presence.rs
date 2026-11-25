//! manual-presence: the shared presence constants and the version gate (change
//! `manual-presence`).

use protocol::presence::{
    DEFAULT_PRESENCE, PRESENCE_STATUS_OFFLINE, PRESENCE_STATUS_ONLINE, PRESENCE_STATUS_UNSPECIFIED,
    Presence,
};
use protocol::wire_transport::{
    PRESENCE_MIN_PROTOCOL_VERSION, PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS,
    server_knows_presence,
};

// spec: presence/manual-status-never-inferred
#[test]
fn exactly_two_values_with_fixed_wire_numbers() {
    assert_eq!(Presence::Online.wire_value(), PRESENCE_STATUS_ONLINE);
    assert_eq!(Presence::Offline.wire_value(), PRESENCE_STATUS_OFFLINE);
    assert_eq!(
        Presence::from_wire(PRESENCE_STATUS_ONLINE),
        Some(Presence::Online)
    );
    assert_eq!(
        Presence::from_wire(PRESENCE_STATUS_OFFLINE),
        Some(Presence::Offline)
    );
    // UNSPECIFIED and unknown values are not a flag: the server ignores them.
    assert_eq!(Presence::from_wire(PRESENCE_STATUS_UNSPECIFIED), None);
    assert_eq!(Presence::from_wire(7), None);
}

// spec: presence/default-status-before-first-set
#[test]
fn the_first_launch_default_is_offline() {
    assert_eq!(DEFAULT_PRESENCE, Presence::Offline);
    assert!(!DEFAULT_PRESENCE.is_online());
}

// spec: presence/broadcasting-a-status-change
#[test]
fn presence_is_gated_to_protocol_version_4() {
    assert_eq!(PROTOCOL_VERSION, 4);
    assert_eq!(SUPPORTED_PROTOCOL_VERSIONS, [1, 2, 3, 4]);
    assert_eq!(PRESENCE_MIN_PROTOCOL_VERSION, 4);
    assert!(server_knows_presence(&[1, 2, 3, 4]));
    assert!(!server_knows_presence(&[1, 2, 3]));
}
