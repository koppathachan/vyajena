# server-runtime Specification

## Purpose
TBD - created by archiving change server-core. Update Purpose after archive.
## Requirements
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

### Requirement: First start
On first start with an empty key directory the server SHALL generate its TLS key
pair and its MLS external sender key pair, write each private key with file mode
0600, and print the TLS key pin (SHA-256 of the SubjectPublicKeyInfo) to its log.
On later starts it SHALL reuse the stored keys, so the pin never changes.

#### Scenario: Fresh server
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
- **WHEN** the server loses power right after Alice's phone receives `Accepted`
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
storage as on the server.

#### Scenario: Developer run
- **WHEN** the owner starts the server with the example local config
- **THEN** it listens on `127.0.0.1:8443`, prints the TLS pin and the bootstrap
  invite, and accepts a pinned TLS connection

### Requirement: Graceful stop
The server SHALL stop cleanly on `SIGTERM` as well as on Ctrl-C: it stops
accepting connections, finishes the transaction in progress, closes the database,
and exits with status 0.

#### Scenario: systemd stops the service
- **WHEN** systemd stops the server while Alice's message is being accepted
- **THEN** either Alice gets `Accepted` and the message is stored, or she gets no
  answer and resends it after the restart; the server exits with status 0

### Requirement: Blob storage degrades gracefully
The server SHALL store image blobs only through the configured `[blob_store]`
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

