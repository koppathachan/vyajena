## MODIFIED Requirements

### Requirement: Content-free wake pings
The server SHALL send a wake ping through FCM or APNs when it adds a **notifiable**
entry to the queue of a device that has no open `Connect` stream and has a push
token. A notifiable entry is an `APPLICATION` envelope that is **not** marked
`silent`. The server SHALL NOT send a wake ping to a device whose undelivered queue
holds only control envelopes (`COMMIT`, `WELCOME`, `PROPOSAL`) and/or `silent`
`APPLICATION` envelopes; those entries stay queued and the device receives them on
its next `Connect`. A device with an **open** stream SHALL still be woken on that
stream for every kind, `silent` envelopes included. The ping SHALL contain no
message content, sender, group id or message count. The server SHALL send at most one
ping per device per 60 seconds.

A sending client SHALL set `Envelope.silent` on an `APPLICATION` envelope whose
encrypted `Content` carries no user-visible message — a profile update, a
group-settings change or a message-deletion tombstone — and SHALL leave it unset on a
text, image, audio or video message and on every control envelope. The server SHALL
read `silent` from the client-set plaintext field; a server that predates the field
treats every envelope as not `silent`.

#### Scenario: Offline device
- **WHEN** an `APPLICATION` message that is not `silent` is queued for a device with
  no open stream
- **THEN** the server sends one wake ping with no content to that device's push token

#### Scenario: Connected device
- **WHEN** a message is queued for a device with an open stream
- **THEN** the server delivers it on the stream and sends no wake ping

#### Scenario: Offline device with only a control envelope
- **WHEN** a `COMMIT`, `WELCOME` or `PROPOSAL` is queued for a device with no open
  stream and no pending notifiable entry
- **THEN** the server sends no wake ping, and the device receives the control
  envelope on its next `Connect`

#### Scenario: Offline device with only a silent application envelope
- **WHEN** a `silent` `APPLICATION` envelope (a profile update, a group-settings
  change or a message-deletion tombstone) is queued for a device with no open stream
  and no pending notifiable entry
- **THEN** the server sends no wake ping, and the device receives the envelope on its
  next `Connect`

#### Scenario: Silent envelope followed by a message
- **WHEN** a `silent` `APPLICATION` envelope is queued for an offline device, and
  later a non-`silent` `APPLICATION` message is queued for it
- **THEN** the server sends no wake ping for the silent envelope and one wake ping
  once the message is queued

#### Scenario: Catch-up sweep ignores a silent-only backlog
- **WHEN** the periodic catch-up sweep runs and an offline device's undelivered queue
  holds only control and/or `silent` `APPLICATION` envelopes
- **THEN** the sweep sends no catch-up ping for that device and spends none of its
  catch-up budget
