## ADDED Requirements

### Requirement: Private logs
The server SHALL NOT write a raw device id, a message body or ciphertext, a session
token or an invite code to its logs or metrics; a device id SHALL appear only as
a short stable hash (`redact`), so lines can be correlated without revealing the
id. The server enforces this at every `tracing` call site.

#### Scenario: A device id is logged as a hash
- **WHEN** Bob's device opens its `Messaging.Connect` stream and the server logs it
- **THEN** the line shows `device=<12-hex-hash>` and nowhere the raw device id

#### Scenario: No secret or content in the logs
- **WHEN** the server handles a `Messaging.Connect` carrying a session token, a
  message with a body, and (for the admin) an invite code
- **THEN** none of the token, the body/ciphertext or the invite code appears in
  any log line or metric event

### Requirement: Connection metrics
The server SHALL emit, for each opened `Messaging.Connect` stream, a structured
event carrying `open_ms` (the time in the Connect task from it starting to the
stream registering), `reopens` (how many times that device's stream has opened
this run) and `active_streams` (the count of live streams), with the device id
redacted. The event is a `tracing` event in the journal; the server exposes no
network endpoint for metrics.

#### Scenario: Metrics are emitted on stream open
- **WHEN** Alice's device opens its `Messaging.Connect` stream
- **THEN** the server emits an event with `open_ms`, `reopens` and `active_streams`,
  and `device` as the redacted hash, and no raw device id

#### Scenario: The active-stream gauge follows real streams
- **WHEN** a second device opens a stream so two are live
- **THEN** the emitted `active_streams` on that open is 2, matching the hub's
  live streams
