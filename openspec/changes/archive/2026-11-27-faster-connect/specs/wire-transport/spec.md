## ADDED Requirements

### Requirement: Session resume
The server SHALL authenticate `Messaging.Connect` from the session token in the
`authorization` metadata alone, accepting a valid, unexpired token of a
non-revoked device whatever connection obtained it, and SHALL NOT tie a token to
the connection that obtained it. The server SHALL reject a `Connect` carrying a
missing, unknown or expired token with `UNAUTHENTICATED`, the same as any other
authenticated RPC. The server enforces this; a client reuses it to open the stream
without first calling `Devices.GetChallenge` and `Devices.Authenticate`.

#### Scenario: Token minted on one connection opens a stream on another
- **WHEN** a device authenticates on one connection, gets a session token, and
  later opens `Messaging.Connect` on a new connection carrying that same token
- **THEN** the server accepts the stream and serves the device, without a fresh
  challenge or `Authenticate` on the new connection

#### Scenario: Expired token on the stream
- **WHEN** a device opens `Messaging.Connect` with a token that has expired
- **THEN** the server rejects the stream with `UNAUTHENTICATED` and serves nothing
