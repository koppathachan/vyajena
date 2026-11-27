## MODIFIED Requirements

### Requirement: Foreground and background
The app SHALL start the core when it opens, SHALL let the core reconnect and catch
up when the app returns to the foreground or the phone's network comes back, and
SHALL show whether the core is connected. The connection indicator SHALL look
different in each state so that connecting never reads as offline: connected is
static in an active colour, connecting is animated in an active colour, offline is
static in a muted colour, and removed is unchanged. While the indicator is
connecting or offline and this device was not removed, tapping it SHALL call the
core's `reconnect_now`, and pulling the chat list down SHALL call `reconnect_now`
too; while connected or removed, tapping the indicator SHALL do nothing. The
client enforces this.

#### Scenario: Back to the app
- **WHEN** Bob switches back to the app after an hour in another app
- **THEN** the core reconnects, catches up in order, and the connection indicator
  turns connected

#### Scenario: Network back
- **WHEN** Bob turns airplane mode off while the core is waiting to retry
- **THEN** the app calls `reconnect_now` and the core reconnects without waiting
  out its backoff

#### Scenario: Connecting does not look offline
- **WHEN** the core is connecting
- **THEN** the indicator animates in the active colour, distinct from the muted
  static look it shows when offline

#### Scenario: Tap to retry
- **WHEN** Bob taps the connection indicator while it shows offline
- **THEN** the app calls `reconnect_now` so the core reconnects without waiting
  out its backoff

#### Scenario: Pull to refresh
- **WHEN** Bob pulls the chat list down
- **THEN** the app calls `reconnect_now`

#### Scenario: Nothing to retry
- **WHEN** Bob taps the indicator while it shows connected, or while this device
  was removed from the group
- **THEN** the app calls nothing
