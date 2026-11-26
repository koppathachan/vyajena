## MODIFIED Requirements

### Requirement: Configuration
The server SHALL read its settings from one TOML file given on the command
line: server id, admin username, listen address, database path and key
directory, plus an optional public address (the relay's `host:port`, written
into invite payloads, defaulting to the listen address) and an optional
`[blob_store]` backend (its `backend`, and for the filesystem backend a stable
`path` where encrypted image blobs are stored; when the table is absent the blob
feature is off). It SHALL also read an optional `[app]` table with
`latest_version` and `min_version` (each optional, a dotted numeric app version
such as `"1.6.0"`, advertised on `ServerInfo` for the update nudge), each
defaulting to unset, so an absent table or field advertises the empty string and
triggers no nudge. It SHALL refuse to start, naming the bad field, if a setting
is missing, invalid or unknown, and SHALL log the effective non-secret settings
once at startup.

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

#### Scenario: App versions configured
- **WHEN** the server starts with `[app]` `latest_version = "1.6.0"` and
  `min_version = "1.4.0"`
- **THEN** it advertises both on `ServerInfo` and logs them among the effective
  settings at startup

#### Scenario: Bad app version refused
- **WHEN** the server starts with `[app]` `min_version = "latest"`
- **THEN** it exits with an error naming `app.min_version` and opens no port
