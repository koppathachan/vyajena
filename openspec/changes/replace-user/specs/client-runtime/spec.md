## MODIFIED Requirements

### Requirement: Connection lifecycle
The core SHALL connect to the relay address from the invite, accept the server only
by its pinned TLS key, authenticate with its device auth key, renew its session token
before it expires, keep one `Connect` stream open with heartbeats, and reconnect with
backoff when the connection drops. When told the network is back (`reconnect_now`),
the core SHALL skip its current backoff wait. When the server answers `Authenticate`
or the stream with `PERMISSION_DENIED`, or sends the account-removal notice
(device-identity: Account-removal notice), the core SHALL emit `Removed` once and stop
connecting. Local history SHALL stay readable after `Removed` until the user resets
the device (client-runtime: Device reset).

#### Scenario: Wrong pin
- **WHEN** the server presents a TLS key that does not match the invite's pin
- **THEN** the core does not connect and reports a server identity error to the UI

#### Scenario: Network drop
- **WHEN** Bob's laptop loses the network for a minute
- **THEN** the core reconnects on its own and catches up in order

#### Scenario: Relay without a tunnel
- **WHEN** the server's tunnel is down, so the relay refuses Bob's TCP connection on
  443 or accepts it and closes it at once, and Alice sends a message meanwhile
- **THEN** Bob's core treats it as a network drop, keeps retrying with backoff, and
  once the tunnel is back connects with the same pin and receives Alice's message

#### Scenario: Reconnect now
- **WHEN** Bob's core is waiting 30 seconds before its next retry and the app calls
  `reconnect_now`
- **THEN** the core connects at once; a call while connected does nothing

#### Scenario: Revoked device
- **WHEN** the admin removes Bob and Bob's phone gets `PERMISSION_DENIED` from
  `Devices.Authenticate`
- **THEN** the core emits `Removed`, stops retrying, and Bob's history stays readable
  until he resets the device

#### Scenario: Told in-band
- **WHEN** Bob's phone is connected and receives the account-removal notice
- **THEN** the core emits `Removed` once, stops connecting, and Bob's history stays
  readable until he resets the device

## ADDED Requirements

### Requirement: Device reset
The core SHALL expose a `reset()` operation that returns the device to a clean
no-account state: it SHALL delete the local account, every group and DM, message
history and pending jobs, and the device's identity, auth, MLS and storage keys from
the OS key store. `reset()` SHALL run only on an explicit caller request, never
automatically, even after `Removed`. After `reset()` the core SHALL hold no account,
so `Devices.Register` with a fresh invite (device-identity: Signup with an invite)
succeeds and the device mints fresh keys. The core performs the reset entirely
locally; no key or content leaves it.

#### Scenario: Start over after removal
- **WHEN** Bob's phone has emitted `Removed` and the caller invokes `reset()`
- **THEN** the core deletes the account, chats, history and key-store keys, and a
  following `register` with a fresh invite succeeds instead of failing
  `AlreadyRegistered`

#### Scenario: Reset needs an explicit call
- **WHEN** the core has emitted `Removed` but no caller has invoked `reset()`
- **THEN** the core never wipes local state on its own, and history stays readable
