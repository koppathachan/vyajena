## MODIFIED Requirements

### Requirement: Single persistent messaging stream
A device SHALL carry all messaging over one `Messaging.Connect` bidirectional
stream per connection: outgoing envelopes, incoming envelopes, send results,
acks, catch-up replay, heartbeats, and the manual-presence frames (`SetPresence`
from the device, `PresenceUpdate` from the server). The server SHALL allow at most
one open `Connect` stream per device and SHALL close the older stream with gRPC
status `ABORTED` when a new one opens.

#### Scenario: Second stream from same device
- **WHEN** a device opens a `Connect` stream while an earlier one is still open
- **THEN** the server ends the earlier stream with `ABORTED` and serves the new one

#### Scenario: Presence rides the same stream
- **WHEN** a device wants to change its user's manual status
- **THEN** it sends a `SetPresence` frame on its open `Connect` stream and receives
  others' `PresenceUpdate` frames on the same stream, with no separate connection
