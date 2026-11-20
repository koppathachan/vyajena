## MODIFIED Requirements

### Requirement: Protocol version negotiation
`Devices.GetServerInfo` SHALL return the server id, the supported protocol
versions, the size limits and the server's external sender public key. The server SHALL
support protocol versions 1 and 2, which behave the same on the stream; a server
that lists version 2 knows `Dms` (direct-messages). The
first frame a client sends on `Connect` SHALL state its protocol version, and the
Server SHALL close the stream with `FAILED_PRECONDITION` if it does not support that
version. A version 2 client SHALL connect with version 2 when the server lists it,
else with version 1, and SHALL offer DMs only when the server lists version 2.

#### Scenario: Unsupported client version
- **WHEN** a client opens `Connect` with a protocol version the server does not support
- **THEN** the server closes the stream with `FAILED_PRECONDITION` and lists the
  supported versions

#### Scenario: Old app on the new server
- **WHEN** a 1.0.0 app opens `Connect` with version 1
- **THEN** the server accepts it and the app works in the main group as before

#### Scenario: New app on an old server
- **WHEN** a version 2 app connects to a server whose `GetServerInfo` lists only
  version 1
- **THEN** the app connects with version 1 and shows no "Message" action
