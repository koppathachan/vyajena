## MODIFIED Requirements

### Requirement: Configuration
The Pi server SHALL read its settings from one TOML file given on the command
line: server id, admin username, listen address, database path and key
directory, plus an optional public address (the relay's `host:port`, written
into invite payloads, defaulting to the listen address) and an optional
`[blob_store]` backend (its `backend`, and for the filesystem backend a stable
`path` where encrypted image blobs are stored; when the table is absent the blob
feature is off). It SHALL refuse to start, naming the bad field, if a setting is
missing, invalid or unknown.

#### Scenario: Missing admin username
- **WHEN** the admin starts the server with a config file that has no admin
  username
- **THEN** the server exits with an error naming `admin_username` and opens no
  port

#### Scenario: Behind the relay
- **WHEN** the server starts with `listen_address = "127.0.0.1:8443"` and
  `public_address = "203.0.113.10:443"`
- **THEN** it accepts connections only on loopback, and the bootstrap invite and
  every `Admin.CreateInvite` payload name `203.0.113.10:443`

#### Scenario: Blob store configured
- **WHEN** the server starts with `[blob_store]` `backend = "filesystem"` and
  `path = "/srv/vyajena-blobs"` on the mounted drive
- **THEN** it stores image blobs there and serves the `Blobs` RPCs

## ADDED Requirements

### Requirement: Blob storage degrades gracefully
The Pi server SHALL store image blobs only through the configured `[blob_store]`
backend, never in the SQLite database or on the main storage. When no backend is
configured, or the configured backend is unavailable (its path not mounted or
not writable, or a remote store unreachable), the server SHALL still start and
serve all messaging and SHALL answer every `Blobs` RPC with `UNAVAILABLE`,
writing no blob to the main storage. The server enforces this.

#### Scenario: Backend path not mounted
- **WHEN** the server starts with the filesystem backend configured but its
  drive not mounted at the stable path
- **THEN** it serves text messaging and answers `Blobs` RPCs with `UNAVAILABLE`,
  and writes no blob to the SD card

#### Scenario: Backend returns
- **WHEN** the drive is mounted at the stable path and the server restarted
- **THEN** the `Blobs` RPCs serve blobs from the backend
