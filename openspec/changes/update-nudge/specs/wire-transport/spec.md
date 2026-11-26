## MODIFIED Requirements

### Requirement: Protocol version negotiation
`Devices.GetServerInfo` SHALL return the server id, the supported protocol
versions, the size limits and the server's external sender public key, and SHALL
also return two optional app-version advisories, `ServerInfo.latest_app_version`
and `ServerInfo.min_app_version` (each a semver string, or empty when the operator
has set none). The first frame a client sends on `Connect` SHALL state its
protocol version, and the server SHALL close the stream with `FAILED_PRECONDITION`
if it does not support that version. The server fills the advisory fields; it does
not act on them, and version negotiation on `Connect` is unchanged by their
presence.

#### Scenario: Unsupported client version
- **WHEN** a client opens `Connect` with a protocol version the server does not support
- **THEN** the server closes the stream with `FAILED_PRECONDITION` and lists the
  supported versions

#### Scenario: App versions advertised
- **WHEN** the server is configured with `latest_version = "1.6.0"` and
  `min_version = "1.4.0"` and Alice's app calls `GetServerInfo`
- **THEN** the `ServerInfo` carries `latest_app_version = "1.6.0"` and
  `min_app_version = "1.4.0"`

#### Scenario: No advisory configured
- **WHEN** the server has no `[app]` versions set and a client calls `GetServerInfo`
- **THEN** `latest_app_version` and `min_app_version` are both the empty string and
  the client shows no update nudge
