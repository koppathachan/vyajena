## MODIFIED Requirements

### Requirement: Configuration
The server SHALL read its settings from one TOML file given on the command
line: server id, admin username, listen address, database path and key
directory, plus an optional public address (the relay's `host:port`, written
into invite payloads, defaulting to the listen address) and an optional
`[blob_store]` backend (its `backend`, and for the filesystem backend a stable
`path` where encrypted image blobs are stored; when the table is absent the blob
feature is off). The server SHALL also read an optional `[limits]` table of
operator-tunable, server-enforced limits — `inactivity_timeout`,
`queue_entry_ttl`, `queue_cap`, `stream_idle_timeout`, `invite_ttl`, `blob_ttl`,
`challenge_ttl`, `dedup_min_retention`, `wake_ping_min_interval`, `catch_up_max`,
`sweep_interval`, `max_blob_store_bytes`, `max_uploads_per_device_per_hour` —
where every field is optional and **defaults to the server's current built-in
value**, so an absent table or field keeps today's behaviour exactly. It SHALL
validate each limit (positive and within sane bounds) and SHALL refuse to start,
naming the bad field, if any setting is missing, invalid or unknown. Client-side
timings are not configurable here.

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

#### Scenario: No limits table keeps the defaults
- **WHEN** the server starts with no `[limits]` table
- **THEN** every limit takes its current built-in value (for example
  `inactivity_timeout` of 7 days and `queue_cap` of 1000), so behaviour is
  unchanged

#### Scenario: A limit is tuned
- **WHEN** the admin sets `[limits]` `inactivity_timeout` to 30 days and restarts
- **THEN** the server removes an idle device only after 30 days, while every
  unset limit keeps its default

#### Scenario: A bad limit refuses start
- **WHEN** the config sets `queue_cap = 0` or an unknown `[limits]` field
- **THEN** the server exits naming that field and opens no port

## ADDED Requirements

### Requirement: Effective configuration is logged at startup
The server SHALL log, once at startup, the effective value of every configurable
limit together with the non-secret base settings (server id, admin username,
listen and public address, whether a blob store is configured and its backend),
so an operator can see what is in force without reading the source. It SHALL NOT
log any secret — no keys, no key-file contents, and never message content. The
server enforces this.

#### Scenario: Operator sees the limits in force
- **WHEN** the server starts
- **THEN** its log shows the effective `inactivity_timeout`, `queue_entry_ttl`,
  `queue_cap` and the other limits, and the server id and addresses, so the
  operator can confirm what is running

#### Scenario: No secret is logged
- **WHEN** the startup configuration is logged
- **THEN** no private key, key-file content or message content appears in the log
