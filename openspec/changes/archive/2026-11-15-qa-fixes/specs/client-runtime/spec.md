## MODIFIED Requirements

### Requirement: Connection lifecycle
The core SHALL connect to the relay address from the invite, accept the Pi only
by its pinned TLS key, authenticate with its device auth key, renew its session
token before it expires, keep one `Connect` stream open with heartbeats, and
reconnect with backoff when the connection drops. When told the network is back
(`reconnect_now`), the core SHALL skip its current backoff wait. When the Pi
answers `Authenticate` or the stream with `PERMISSION_DENIED`, the core SHALL emit
`Removed` once and stop connecting.

#### Scenario: Wrong pin
- **WHEN** the Pi presents a TLS key that does not match the invite's pin
- **THEN** the core does not connect and reports a server identity error to the UI

#### Scenario: Network drop
- **WHEN** Bob's laptop loses the network for a minute
- **THEN** the core reconnects on its own and catches up in order

#### Scenario: Reconnect now
- **WHEN** Bob's core is waiting 30 seconds before its next retry and the app
  calls `reconnect_now`
- **THEN** the core connects at once; a call while connected does nothing

#### Scenario: Revoked device
- **WHEN** the admin removes Bob and Bob's phone gets `PERMISSION_DENIED` from
  `Devices.Authenticate`
- **THEN** the core emits `Removed`, stops retrying, and Bob's history stays
  readable

### Requirement: Events in order
The core SHALL deliver events to the UI in `server_seq` order through one event
stream: received messages, profile and group-setting changes (with who changed
the group settings), gap notices, removal and rejoin notices, membership changes
(devices added and removed, and the committer's user), the removed-device event,
and identity-key-changed warnings.

#### Scenario: Back from holiday
- **WHEN** Bob's laptop returns after 10 days and is re-added
- **THEN** the UI gets a removed-for-inactivity event with the time messages stop,
  then the messages from after the re-add, in order

#### Scenario: Tablet added
- **WHEN** Bob's phone commits the Add of Bob's tablet
- **THEN** every member's core emits `MembersChanged` with the tablet in `added`
  and `bob@pi` as the committer's user
