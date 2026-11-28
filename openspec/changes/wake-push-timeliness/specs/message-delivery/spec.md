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
message content, sender, group id or message count.

The per-device wake-ping rate limit is **reason-aware**. A wake ping for a
newly-queued notifiable message — the write path, and the stream-loss re-ping that
exists to deliver a pending message — SHALL be allowed once at least
`NEW_MESSAGE_PING_MIN_INTERVAL` (10 seconds) have passed since the last such
*message* ping to that device, and SHALL NOT be suppressed by a catch-up or
own-access nudge sent in between. A **nudge** — a periodic catch-up sweep ping or an
own-access admin-event ping — SHALL be sent at most once per device per 60 seconds
and SHALL NOT reset or consume the message interval. Because the wake payload carries
a fixed `collapse_key`, repeated pings within these intervals collapse to a single
notification on the device.

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

#### Scenario: A nudge does not starve a real message
- **WHEN** a catch-up or own-access nudge was sent to an offline device less than
  60 seconds ago, and a new notifiable message is then queued for it at least
  10 seconds after the last message ping
- **THEN** the server sends a wake ping for the message; the recent nudge does not
  suppress it

#### Scenario: Message floor
- **WHEN** two notifiable messages are queued for an offline device less than
  10 seconds apart
- **THEN** the server sends one wake ping, and the second message's ping is sent only
  once 10 seconds have passed since the first

#### Scenario: Nudge cadence unchanged
- **WHEN** two catch-up sweeps would ping the same offline device less than
  60 seconds apart
- **THEN** the server sends at most one nudge ping to that device in the 60 seconds
