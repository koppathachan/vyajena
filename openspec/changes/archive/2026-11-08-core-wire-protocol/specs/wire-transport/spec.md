## ADDED Requirements

### Requirement: Protobuf contract is the source of truth
The wire protocol SHALL be defined by the `.proto` files in the `vyajena.v1`
package under `proto/`. Clients and the server SHALL use code generated from those
files, and no wire message SHALL exist that is not defined there.

#### Scenario: Breaking change is caught
- **WHEN** a pull request removes or renumbers a field in a `vyajena.v1` message
- **THEN** the CI breaking-change check fails the build

#### Scenario: Additive change is allowed
- **WHEN** a pull request adds a new optional field with a new field number
- **THEN** the CI breaking-change check passes

### Requirement: gRPC over HTTP/2 and pinned TLS on TCP 443
All client–server traffic SHALL be gRPC over HTTP/2 over TLS 1.3 on TCP port 443,
with TLS terminated on the server. The client SHALL authenticate the server only by
comparing the SHA-256 hash of the server's TLS SubjectPublicKeyInfo with the pin it
received in its invite, and SHALL NOT rely on any certificate authority or
domain name.

#### Scenario: Pinned key matches
- **WHEN** the client connects and the server's TLS public key hash equals the pin
- **THEN** the TLS handshake completes and gRPC calls proceed

#### Scenario: Pinned key does not match
- **WHEN** the server presents a TLS key whose hash differs from the pin
- **THEN** the client aborts the connection before sending any gRPC request and
  reports a server identity error

### Requirement: Session authentication
The server SHALL require a valid session token in the `authorization` metadata on
every RPC except `Devices.GetServerInfo`, `Devices.Register`,
`Devices.GetChallenge` and `Devices.Authenticate`. A device SHALL obtain a token by signing
the server's challenge nonce with its device auth key, over the context string
`vyajena2-auth-v1`, the nonce and the TLS exporter value of the current
connection. The nonce SHALL be 32 random bytes, single-use, and valid for 60
seconds after it is issued. The TLS exporter value SHALL be the 32-byte TLS 1.3
exporter output with label `EXPORTER-vyajena2-auth-v1` and an empty context.
Tokens SHALL expire after at most 1 hour.

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

### Requirement: Single persistent messaging stream
A device SHALL carry all messaging over one `Messaging.Connect` bidirectional
stream per connection: outgoing envelopes, incoming envelopes, send results,
acks, catch-up replay and heartbeats. The server SHALL allow at most one open
`Connect` stream per device and SHALL close the older stream with gRPC status
`ABORTED` when a new one opens.

#### Scenario: Second stream from same device
- **WHEN** a device opens a `Connect` stream while an earlier one is still open
- **THEN** the server ends the earlier stream with `ABORTED` and serves the new one

### Requirement: Heartbeats
While a `Connect` stream is idle the client SHALL send a heartbeat at least every
25 seconds, and the server SHALL reply to each. The server SHALL close a stream that has
received nothing for 90 seconds.

#### Scenario: Silent client
- **WHEN** the server receives no frame on a stream for 90 seconds
- **THEN** the server closes the stream and treats the device as offline

### Requirement: Size limits
The ciphertext of an `APPLICATION` envelope SHALL NOT exceed 65,536 bytes.
Handshake envelopes (`COMMIT` and `PROPOSAL`) SHALL NOT exceed 262,144 bytes,
encoded as sent; for a `COMMIT` this includes the Welcome it carries. The client SHALL refuse to send an envelope over its limit, and the server
SHALL reject one with `TOO_LARGE`. The server SHALL advertise both limits in
`GetServerInfo`.

#### Scenario: Oversized application message
- **WHEN** the server receives an `APPLICATION` envelope whose ciphertext is 65,537
  bytes
- **THEN** the server rejects it with reason `TOO_LARGE` and stores nothing

#### Scenario: Commit with a large Welcome
- **WHEN** a `COMMIT` envelope is 200,000 bytes and its Welcome bundle adds
  70,000 bytes
- **THEN** the server rejects the envelope, Welcome included, with `TOO_LARGE`

#### Scenario: Client-side check
- **WHEN** encrypting a message would produce more than 65,536 bytes of ciphertext
- **THEN** the Rust core returns an error to the UI and sends nothing

### Requirement: Protocol version negotiation
`Devices.GetServerInfo` SHALL return the server id, the supported protocol
versions, the size limits and the server's external sender public key. The first
frame a client sends on `Connect` SHALL state its protocol version, and the server
SHALL close the stream with `FAILED_PRECONDITION` if it does not support that
version.

#### Scenario: Unsupported client version
- **WHEN** a client opens `Connect` with a protocol version the server does not support
- **THEN** the server closes the stream with `FAILED_PRECONDITION` and lists the
  supported versions
