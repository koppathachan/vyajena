## ADDED Requirements

### Requirement: Configuration
The Pi server SHALL read its settings from one TOML file given on the command
line: server id, admin username, listen address, database path and key
directory. It SHALL refuse to start, naming the bad field, if a setting is
missing or invalid.

#### Scenario: Missing admin username
- **WHEN** the admin starts the server with a config file that has no admin
  username
- **THEN** the server exits with an error naming `admin_username` and opens no
  port

### Requirement: First start
On first start with an empty key directory the server SHALL generate its TLS key
pair and its MLS external sender key pair, write each private key with file mode
0600, and print the TLS key pin (SHA-256 of the SubjectPublicKeyInfo) to its log.
On later starts it SHALL reuse the stored keys, so the pin never changes.

#### Scenario: Fresh Pi
- **WHEN** the server starts with an empty key directory
- **THEN** it creates both key files with mode 0600 and logs the TLS pin

#### Scenario: Restart keeps the pin
- **WHEN** the server is restarted
- **THEN** it logs the same TLS pin as before

### Requirement: Durable storage
The server SHALL store all state in one SQLite database in WAL mode with full
synchronous commits, with file mode 0600, and SHALL apply schema migrations at
start. It SHALL send `Accepted` only after the transaction holding the envelope
and its queue and log entries has committed.

#### Scenario: Power cut after Accepted
- **WHEN** the Pi loses power right after Alice's phone receives `Accepted`
- **THEN** after restart the envelope is still queued for every recipient

### Requirement: Push provider interface
The server SHALL send wake pings through a push provider interface. When no real
provider is configured it SHALL use a logging provider that records only the
recipient device id and platform, never content, sender or group.

#### Scenario: Local wake ping
- **WHEN** a message is queued for Bob's offline phone on a server without a push
  provider configured
- **THEN** the server logs one wake ping for Bob's phone's device id and sends
  nothing

### Requirement: Running locally
The server SHALL run on a developer machine without root, listening on a
configurable unprivileged port, with the same TLS pinning, authentication and
storage as on the Pi.

#### Scenario: Developer run
- **WHEN** the owner starts the server with the example local config
- **THEN** it listens on `127.0.0.1:8443`, prints the TLS pin and the bootstrap
  invite, and accepts a pinned TLS connection
