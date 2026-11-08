## ADDED Requirements

### Requirement: Envelope
Every message on the `Connect` stream SHALL be carried in an `Envelope` with a
client-generated 16-byte `msg_id`, the `group_id`, the `epoch`, a `kind`
(`APPLICATION`, `COMMIT`, `WELCOME` or `PROPOSAL`) and the MLS message bytes as
`ciphertext`. `WELCOME` envelopes SHALL also list their recipient devices. The Pi
SHALL set `sender` from the authenticated device and SHALL add a per-group
`server_seq` and an `accepted_at` time when it accepts the envelope.

#### Scenario: Sender cannot be spoofed
- **WHEN** a device sends an envelope
- **THEN** recipients see the sender as the device that authenticated the stream,
  regardless of anything the client put in the envelope

### Requirement: Framing matches the envelope
The Pi SHALL parse the MLS framing of every envelope and SHALL reject it with
`MALFORMED` if its group id, epoch or content type do not match the envelope's
`group_id`, `epoch` and `kind`. `APPLICATION` envelopes SHALL carry an MLS
`PrivateMessage`; `COMMIT` and `PROPOSAL` envelopes SHALL carry an MLS
`PublicMessage`.

#### Scenario: Mismatched epoch
- **WHEN** an envelope says epoch 7 but its MLS header says epoch 6
- **THEN** the Pi rejects it with `MALFORMED`

#### Scenario: Encrypted commit
- **WHEN** a `COMMIT` envelope carries a `PrivateMessage`
- **THEN** the Pi rejects it with `MALFORMED`

### Requirement: Accepted means durable
The Pi SHALL send `Accepted` for an envelope only after it has durably written the
envelope and every queue or log entry it creates. The client SHALL show a message
as sending until it receives `Accepted` for its `msg_id`, then as accepted.

#### Scenario: Pi crashes before writing
- **WHEN** the Pi crashes after receiving an envelope but before committing it to
  disk
- **THEN** the client never receives `Accepted` and retries on reconnect

### Requirement: Idempotent sends
The Pi SHALL deduplicate accepted envelopes on (sender device, `msg_id`) for at
least 7 days; rejected envelopes SHALL leave no dedup record. A retry with the same `msg_id` and identical ciphertext SHALL receive the
original result without creating new entries. A retry with the same `msg_id` and
different ciphertext SHALL be rejected with `DUPLICATE_MISMATCH`.

#### Scenario: Retry after lost ack
- **WHEN** a client resends an envelope it already sent because the `Accepted`
  result was lost
- **THEN** the Pi returns the original `server_seq` and recipients get the message
  once

### Requirement: Strict epoch rule
The Pi SHALL reject any envelope whose epoch is not the group's current epoch with
`STALE_EPOCH`, returning the current epoch. On `STALE_EPOCH` the client SHALL
process the pending commits, re-encrypt in the current epoch and resend with
the same `msg_id`.

#### Scenario: Message sent across a commit
- **WHEN** a device sends an application message in epoch 4 after the Pi accepted
  a commit moving the group to epoch 5
- **THEN** the Pi rejects it with `STALE_EPOCH` and current epoch 5

#### Scenario: Racing commits
- **WHEN** two devices each send a commit for epoch 4
- **THEN** the Pi accepts the first, moves the group to epoch 5, and rejects the
  second with `STALE_EPOCH`

### Requirement: Per-device queues
For each accepted `APPLICATION` envelope the Pi SHALL create one queue entry per
active member device of the group other than the sender device. A queue SHALL
hold at most 1,000 application entries. When an envelope would exceed a device's
cap, the Pi SHALL accept the envelope for everyone else, SHALL NOT queue it for
that device, and SHALL mark that device inactive (see group-membership). The Pi
SHALL never reject a send because of another device's queue.

#### Scenario: Sender's other devices receive the message
- **WHEN** Alice sends from her phone
- **THEN** her tablet receives the message like every other member device

#### Scenario: Full queue
- **WHEN** one recipient device already has 1,000 queued application entries
- **THEN** the send is accepted, every other device receives it, and the full
  device is marked inactive

### Requirement: Commit log
The Pi SHALL append every accepted `COMMIT` and `PROPOSAL` to the group's commit
log and SHALL keep a delivery cursor per member device. `COMMIT`, `PROPOSAL` and
`WELCOME` entries SHALL be exempt from expiry and the queue cap, and SHALL be
deleted once every device that must receive them has acknowledged them.

#### Scenario: Commit kept for an offline member
- **WHEN** a member device has been offline for 5 days
- **THEN** every commit since its cursor is still available to it

### Requirement: Ordered delivery and catch-up
The Pi SHALL deliver to each device its commit log entries, welcomes and queued
application entries merged in `server_seq` order. A reconnecting device SHALL
receive everything after its cursor in that order before any live traffic.

#### Scenario: Catch-up across an epoch change
- **WHEN** a device was offline while message M1 (epoch 4), commit C (epoch 4 to 5)
  and message M2 (epoch 5) were accepted
- **THEN** on reconnect it receives M1, then C, then M2

### Requirement: Acknowledgement
A client SHALL acknowledge with the highest `server_seq` it has durably processed,
per group. The Pi SHALL then delete that device's queue entries up to that
sequence, advance its commit-log cursor, and not redeliver them.

#### Scenario: Ack deletes queue entries
- **WHEN** a device acknowledges `server_seq` 120
- **THEN** the Pi deletes that device's queue entries up to 120 and the next
  reconnect starts after 120

### Requirement: Expiry and gap notice
The Pi SHALL delete `APPLICATION` queue entries 7 days after `accepted_at`, as a
backstop for devices that connect but never acknowledge. When a device next
connects after entries for it expired, the Pi SHALL send it a gap notice with the
time of the newest expired entry, so the client can show "messages before X
expired". Devices that stay offline are handled by inactivity removal
(group-membership) before this applies.

#### Scenario: Device that never acks
- **WHEN** a connected device has not acknowledged an entry for 7 days
- **THEN** the Pi deletes the entry and sends the device a gap notice

### Requirement: Content-free wake pings
The Pi SHALL send a wake ping through FCM or APNs when it adds an entry to the
queue of a device that has no open `Connect` stream and has a push token.
The ping SHALL contain no message content, sender, group id or message count. The
Pi SHALL send at most one ping per device per 60 seconds.

#### Scenario: Offline device
- **WHEN** a message is queued for a device with no open stream
- **THEN** the Pi sends one wake ping with no content to that device's push token

#### Scenario: Connected device
- **WHEN** a message is queued for a device with an open stream
- **THEN** the Pi delivers it on the stream and sends no wake ping
