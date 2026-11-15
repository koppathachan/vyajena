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

#### Scenario: Relay without a tunnel
- **WHEN** the Pi's tunnel is down, so the relay refuses Bob's TCP connection on
  443 or accepts it and closes it at once, and Alice sends a message meanwhile
- **THEN** Bob's core treats it as a network drop, keeps retrying with backoff,
  and once the tunnel is back connects with the same pin and receives Alice's
  message

#### Scenario: Reconnect now
- **WHEN** Bob's core is waiting 30 seconds before its next retry and the app
  calls `reconnect_now`
- **THEN** the core connects at once; a call while connected does nothing

#### Scenario: Revoked device
- **WHEN** the admin removes Bob and Bob's phone gets `PERMISSION_DENIED` from
  `Devices.Authenticate`
- **THEN** the core emits `Removed`, stops retrying, and Bob's history stays
  readable
