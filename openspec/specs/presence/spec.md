# presence Specification

## Purpose
TBD - created by archiving change manual-presence. Update Purpose after archive.
## Requirements
### Requirement: Manual status, never inferred
Each user SHALL have a manual presence status that is exactly one of two values,
Online or Offline (proto `PRESENCE_STATUS_ONLINE` / `PRESENCE_STATUS_OFFLINE`), set
only by that user's own action. Neither the server nor the client SHALL derive it
from connection state, heartbeats, reads, typing or any other activity. The status
persists until the user changes it, regardless of whether any of the user's devices
is connected. The server and the client both enforce this.

#### Scenario: Online survives going offline
- **WHEN** Alice sets her status to `PRESENCE_STATUS_ONLINE` and then closes the app
  so all her devices disconnect
- **THEN** the server keeps her stored status Online and other users still see her
  as Online until she changes it

#### Scenario: Connection is not presence
- **WHEN** Bob's stream goes idle and the server marks his device offline for
  delivery after 90 seconds
- **THEN** the server does not change Bob's manual status and sends no
  `PresenceUpdate` about him

### Requirement: Setting your status
A user's device SHALL set the user's status by sending a `ClientFrame.SetPresence`
with the chosen `PresenceStatus` on the `Messaging.Connect` stream, any time after
`Hello`. The server SHALL record the value against the sending device's user and
stamp `updated_at` itself; the server SHALL ignore a `SetPresence` carrying
`PRESENCE_STATUS_UNSPECIFIED`. The server sets the status from nothing but a
`SetPresence` (or the first-launch default). The server enforces this.

#### Scenario: Alice goes Offline
- **WHEN** Alice's phone sends `SetPresence{ status: PRESENCE_STATUS_OFFLINE }`
- **THEN** the server stores Alice's status as Offline with a fresh `updated_at`

#### Scenario: Any of the user's devices may set it
- **WHEN** Alice's tablet sends `SetPresence{ ONLINE }` while her phone is also
  connected
- **THEN** the server records Alice's user status as Online (status is per user, not
  per device)

### Requirement: Broadcasting a status change
When a user's status changes, the server SHALL send a `ServerFrame.PresenceUpdate`
carrying that user's `UserId`, the new `PresenceStatus` and `updated_at` to every
connected device of every **other** user who shares a conversation with them — the
lobby, a named group, or a DM. The server SHALL NOT send it to a user who shares no
conversation with the setter, and SHALL send it only to streams that negotiated
protocol version 4 or above. The server enforces this.

#### Scenario: A co-member is told
- **WHEN** Alice sets Online and Bob, who is with her in the lobby, is connected
- **THEN** Bob's device receives `PresenceUpdate{ user: alice, status: ONLINE }`

#### Scenario: A non-sharing user is not told
- **WHEN** Alice sets Online and Carol shares no lobby, group or DM with Alice
- **THEN** the server sends Carol no `PresenceUpdate` about Alice

#### Scenario: Old client is not sent presence
- **WHEN** a co-member's device negotiated protocol version 3
- **THEN** the server sends it no `PresenceUpdate`

### Requirement: Status snapshot on connect
On each `Messaging.Connect`, before live traffic, the server SHALL send one
`PresenceUpdate` carrying the current status of each user the connecting user may
see (the broadcast scope above), so the device can render every visible avatar's dot
immediately. The server enforces this.

#### Scenario: Reconnect gets current statuses
- **WHEN** Bob reconnects and may see Alice (Online) and Dave (Offline)
- **THEN** before any envelope, his stream receives `PresenceUpdate{ alice, ONLINE }`
  and `PresenceUpdate{ dave, OFFLINE }`

### Requirement: Latest value wins
A `PresenceUpdate` SHALL carry state, not a queued message: there is no `Ack` for it
and the server keeps no per-device presence queue. A client SHALL keep only the
latest status per user and SHALL apply a `PresenceUpdate` only if its `updated_at`
is newer than the one it already holds for that user. The client enforces this.

#### Scenario: Snapshot does not overwrite a newer live change
- **WHEN** Bob's connect snapshot reports Alice Offline at `t1`, and a live
  `PresenceUpdate` reports Alice Online at `t2 > t1` arrives
- **THEN** Bob shows Alice Online, and a later stray `t1` frame does not revert it

### Requirement: Durable per-user status storage
The server SHALL store each user's manual status durably in its SQLite store so it
survives restart, and SHALL NOT clear it when the user's last device disconnects.
The server is the authority for the stored value and writes it only from a
`SetPresence` by that user's authenticated device or from the first-launch default.
The server enforces this.

#### Scenario: Status survives a server restart
- **WHEN** Alice is stored Offline and the server restarts
- **THEN** after restart Alice is still Offline and her co-members' next snapshot
  reports Offline

### Requirement: Default status before first set
A user who has never sent a `SetPresence` SHALL have the owner-chosen default status
(the `protocol/` default constant), applied by the server at `Devices.Register` and
reported by any snapshot for that user. The server enforces this.

#### Scenario: Brand-new user
- **WHEN** a freshly registered user has never set a status and a co-member connects
- **THEN** the snapshot reports that user at the default status

### Requirement: Toggle your status on the Profile screen
The app SHALL offer a sliding toggle (a `Switch`) on the Profile screen for the
user's own status: turning it on sets Online and turning it off sets Offline
(sending the matching `SetPresence`), and the toggle SHALL reflect the user's
current manual flag. The bottom-navigation profile-tab avatar SHALL carry a small
presence dot driven by this manual flag — green when Online, red when Offline — and
never by the connection state. The app enforces this.

#### Scenario: Toggle on to go Online
- **WHEN** the user turns the Profile presence toggle on
- **THEN** the core sends `SetPresence{ ONLINE }` and the profile-tab avatar's dot
  turns green

#### Scenario: Toggle off to go Offline
- **WHEN** the user turns the Profile presence toggle off
- **THEN** the core sends `SetPresence{ OFFLINE }` and the profile-tab avatar's dot
  turns red

### Requirement: Presence indicator on avatars
The app SHALL show a green (Online) or red (Offline) presence dot or ring on a
user's avatar in chat-list rows, the Contacts tab and conversation headers, for each
user whose status the viewer may see. When a user's status is not known to the
device, the app SHALL show no presence dot rather than guess a colour. The app
enforces this.

#### Scenario: Dot reflects a received status
- **WHEN** Bob has received `PresenceUpdate{ alice, ONLINE }`
- **THEN** Alice's avatar shows a green dot in Bob's chats list, Contacts and the DM
  header

#### Scenario: Unknown status shows no dot
- **WHEN** Bob has received no status for Dave
- **THEN** Dave's avatar shows no presence dot

