## MODIFIED Requirements

### Requirement: Configuration
The Pi server SHALL read its settings from one TOML file given on the command
line: server id, admin username, listen address, database path and key
directory, plus an optional public address (the relay's `host:port`, written
into invite payloads, defaulting to the listen address) and an optional
`review_mode` (true or false, default false; review-server: Review mode). It
SHALL refuse to start, naming the bad field, if a setting is missing, invalid or
unknown.

#### Scenario: Missing admin username
- **WHEN** the admin starts the server with a config file that has no admin
  username
- **THEN** the server exits with an error naming `admin_username` and opens no
  port

#### Scenario: Review mode is off by default
- **WHEN** the config has no `review_mode`
- **THEN** the server runs as the real server; a `review_mode` that is not true
  or false stops it with an error naming `review_mode`
