## MODIFIED Requirements

### Requirement: Configuration
The server SHALL read its settings from one TOML file given on the command
line: server id, admin username, listen address, database path and key
directory, plus an optional public address (the relay's `host:port`, written
into invite payloads, defaulting to the listen address), an optional
`[blob_store]` backend (its `backend`, and for the filesystem backend a stable
`path` where encrypted image blobs are stored; when the table is absent the blob
feature is off) and an optional `[review]` table (`reusable_admin_invite`, a
boolean, default false; device-identity: Reusable review admin invite). When
`reusable_admin_invite` is true the server SHALL log a `WARN` at every start
saying the admin seat can be taken over with the reusable invite. It SHALL refuse
to start, naming the bad field, if a setting is missing, invalid or unknown.

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

#### Scenario: Review flag on
- **WHEN** the server starts with `[review]` `reusable_admin_invite = true`
- **THEN** it logs a `WARN` naming the reusable admin invite and starts

#### Scenario: Unknown review setting
- **WHEN** the server starts with `[review]` `open = true`
- **THEN** it exits with an error naming `review.open` and opens no port
