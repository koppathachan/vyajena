## MODIFIED Requirements

### Requirement: Content-free wake pings
The server SHALL send a wake ping through FCM or APNs when it adds an
`APPLICATION` entry to the queue of a device that has no open `Connect` stream
and has a push token. The server SHALL NOT send a wake ping to a device whose
undelivered queue holds only control envelopes (`COMMIT`, `WELCOME` or
`PROPOSAL`); those entries stay queued and the device receives them on its next
`Connect`. A device with an **open** stream SHALL still be woken on that stream
for every kind. The ping SHALL contain no message content, sender, group id or
message count. The server SHALL send at most one ping per device per 60 seconds.

#### Scenario: Offline device
- **WHEN** an `APPLICATION` message is queued for a device with no open stream
- **THEN** the server sends one wake ping with no content to that device's push token

#### Scenario: Connected device
- **WHEN** a message is queued for a device with an open stream
- **THEN** the server delivers it on the stream and sends no wake ping

#### Scenario: Offline device with only a control envelope
- **WHEN** a `COMMIT`, `WELCOME` or `PROPOSAL` is queued for a device with no
  open stream and no pending `APPLICATION` entry
- **THEN** the server sends no wake ping, and the device receives the control
  envelope on its next `Connect`

#### Scenario: Control envelope followed by a message
- **WHEN** a control envelope is queued for an offline device, and later an
  `APPLICATION` message is queued for it
- **THEN** the server sends no wake ping for the control envelope and one wake
  ping once the `APPLICATION` message is queued
