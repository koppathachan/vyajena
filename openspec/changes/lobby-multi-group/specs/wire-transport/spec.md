## MODIFIED Requirements

### Requirement: Protocol version negotiation
`Devices.GetServerInfo` SHALL return the server id, the supported protocol
versions, the size limits and the server's external sender public key. The server
SHALL support protocol versions 1, 2 and 3, which behave the same on the stream; a
server that lists version 2 knows `Dms` (direct-messages), and one that lists
version 3 also knows named groups (multi-group). The first frame a client sends on
`Connect` SHALL state its protocol version, and the server SHALL close the stream
with `FAILED_PRECONDITION` if it does not support that version. A client SHALL
connect with the highest version it and the server share, and SHALL offer named
groups only when the server lists version 3 and DMs only when it lists version 2.

#### Scenario: Unsupported client version
- **WHEN** a client opens `Connect` with a protocol version the server does not support
- **THEN** the server closes the stream with `FAILED_PRECONDITION` and lists the
  supported versions

#### Scenario: Old app on the new server
- **WHEN** a version 1 or version 2 app opens `Connect` with its version
- **THEN** the server accepts it and the app works in the lobby and DMs as before,
  without named groups

#### Scenario: New app on an old server
- **WHEN** a version 3 app connects to a server whose `GetServerInfo` lists only
  versions 1 and 2
- **THEN** the app connects with version 2 and shows no "Create group" action
