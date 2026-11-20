## MODIFIED Requirements

### Requirement: Session authentication
The server SHALL require a valid session token in the `authorization` metadata on
every RPC except `Devices.GetServerInfo`, `Devices.Register`,
`Devices.GetChallenge` and `Devices.Authenticate`. A device SHALL obtain a token by signing
the server's challenge nonce with its device auth key, over the context string
`vyajena2-auth-v1`, the nonce and the TLS exporter value of the current
connection. The nonce SHALL be 32 random bytes, single-use, and valid for 60
seconds after it is issued. The TLS exporter value SHALL be the 32-byte TLS 1.3
exporter output with label `EXPORTER-vyajena2-auth-v1` and an empty context.
Tokens SHALL expire after at most 6 hours. The server SHALL answer `Authenticate` from
a revoked device with `PERMISSION_DENIED`, and only after its signature checks
out, so no one without the device's auth key learns whether it is revoked.

#### Scenario: Successful authentication
- **WHEN** a registered, non-revoked device returns a valid signature over a fresh
  challenge
- **THEN** the server returns a session token and its expiry time

#### Scenario: Replayed challenge
- **WHEN** a device submits a signature for a nonce that was already used or has
  expired
- **THEN** the server rejects the request with `UNAUTHENTICATED`

#### Scenario: Slow answer
- **WHEN** a device submits a signature for a nonce issued 61 seconds earlier
- **THEN** the server rejects the request with `UNAUTHENTICATED`

#### Scenario: Signature from another connection
- **WHEN** a signature was computed over the TLS exporter value of a different
  connection than the one carrying `Authenticate`
- **THEN** the server rejects the request with `UNAUTHENTICATED`

#### Scenario: Missing or expired token
- **WHEN** a client calls an authenticated RPC without a token or with an expired
  one
- **THEN** the server rejects the call with `UNAUTHENTICATED`

#### Scenario: Token lifetime
- **WHEN** Alice's device authenticates
- **THEN** the expiry the server returns is six hours after its issue time, never
  later

#### Scenario: Revoked device authenticates
- **WHEN** the admin removed Bob and Bob's phone returns a valid signature over a
  fresh challenge
- **THEN** the server rejects `Devices.Authenticate` with `PERMISSION_DENIED`

#### Scenario: Bad signature for a revoked device
- **WHEN** someone without Bob's auth key calls `Devices.Authenticate` for Bob's
  revoked phone with a wrong signature
- **THEN** the server rejects the request with `UNAUTHENTICATED`, the same as for any
  bad signature
