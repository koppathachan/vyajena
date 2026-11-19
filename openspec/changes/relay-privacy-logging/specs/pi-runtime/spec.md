## ADDED Requirements

### Requirement: Private logs
The Pi SHALL NOT write a raw device id, a message body or ciphertext, a session
token or an invite code to its logs or metrics; a device id SHALL appear only as
a short stable hash (`redact`), so lines can be correlated without revealing the
id. The Pi enforces this at every `tracing` call site.

#### Scenario: A device id is logged as a hash
- **WHEN** Bob's device opens its `Messaging.Connect` stream and the Pi logs it
- **THEN** the line shows `device=<12-hex-hash>` and nowhere the raw device id

#### Scenario: No secret or content in the logs
- **WHEN** the Pi handles a `Messaging.Connect` carrying a session token, a
  message with a body, and (for the admin) an invite code
- **THEN** none of the token, the body/ciphertext or the invite code appears in
  any log line or metric event

### Requirement: Connection metrics
The Pi SHALL emit, for each opened `Messaging.Connect` stream, a structured
event carrying the connect, auth and stream-open durations, the number of times
that device's stream has (re)opened this run, and the current count of active
streams, with the device id redacted. The event is a `tracing` event in the
journal; the Pi exposes no network endpoint for metrics.

#### Scenario: Timings are emitted on stream open
- **WHEN** Alice's device authenticates and opens its stream
- **THEN** the Pi emits an event with `auth_ms`, `stream_open_ms`, a reopen count
  and `active_streams`, and `device` as the redacted hash

#### Scenario: The active-stream gauge follows real streams
- **WHEN** a second device opens a stream and then the first disconnects
- **THEN** the emitted `active_streams` rises to 2 and later falls, matching the
  hub's live streams
