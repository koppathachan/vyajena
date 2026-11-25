//! manual-presence: the manual online/offline flag both sides agree on (change
//! `manual-presence`).
//!
//! The wire enum `PresenceStatus` lives in the generated protobuf (`pb`); its
//! integer values are fixed here so the server and the device core name the same
//! Online/Offline values without depending on each other's generated code, and
//! the first-launch default (D2) is one shared constant.

/// A user's manual presence flag. Exactly two values for the MVP; derived from
/// nothing — only the user's own `SetPresence` (or the first-launch default)
/// sets it (manual-presence: Manual status, never inferred).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// The user chose to appear online (shown green).
    Online,
    /// The user chose to appear offline (shown red).
    Offline,
}

/// The wire value of `PRESENCE_STATUS_UNSPECIFIED`. Ignored by the server: it
/// never sets a user's flag from it (manual-presence: Setting your status).
pub const PRESENCE_STATUS_UNSPECIFIED: i32 = 0;
/// The wire value of `PRESENCE_STATUS_ONLINE`.
pub const PRESENCE_STATUS_ONLINE: i32 = 1;
/// The wire value of `PRESENCE_STATUS_OFFLINE`.
pub const PRESENCE_STATUS_OFFLINE: i32 = 2;

/// The owner-chosen default status a user has before they ever send a
/// `SetPresence` (manual-presence: Default status before first set, D2). The
/// owner chose **Offline** — private by default, so a user who never touched the
/// toggle is not advertised as online. The server stores it at
/// `Devices.Register` and reports it in any snapshot for such a user.
pub const DEFAULT_PRESENCE: Presence = Presence::Offline;

impl Presence {
    /// The wire value (`PresenceStatus` enum) for this flag.
    pub fn wire_value(self) -> i32 {
        match self {
            Presence::Online => PRESENCE_STATUS_ONLINE,
            Presence::Offline => PRESENCE_STATUS_OFFLINE,
        }
    }

    /// The flag a wire `PresenceStatus` value stands for, or `None` for
    /// `UNSPECIFIED` (which the server ignores) or any unknown value.
    pub fn from_wire(value: i32) -> Option<Self> {
        match value {
            PRESENCE_STATUS_ONLINE => Some(Presence::Online),
            PRESENCE_STATUS_OFFLINE => Some(Presence::Offline),
            _ => None,
        }
    }

    /// Whether this is Online.
    pub fn is_online(self) -> bool {
        matches!(self, Presence::Online)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_round_trip() {
        for p in [Presence::Online, Presence::Offline] {
            assert_eq!(Presence::from_wire(p.wire_value()), Some(p));
        }
        assert_eq!(Presence::from_wire(PRESENCE_STATUS_UNSPECIFIED), None);
        assert_eq!(Presence::from_wire(99), None);
    }

    #[test]
    fn default_is_offline() {
        assert_eq!(DEFAULT_PRESENCE, Presence::Offline);
        assert!(!DEFAULT_PRESENCE.is_online());
    }
}
