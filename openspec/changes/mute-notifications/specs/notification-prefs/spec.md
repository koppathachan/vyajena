## ADDED Requirements

### Requirement: Mute a conversation
A user SHALL be able to mute a single conversation — a DM, the lobby, or a named
group — by its `group_id`, and to unmute it, so that a muted conversation raises
no wake push and no in-app notification on any of the user's devices while its
messages still deliver and still show as unread. Muting SHALL call
`Prefs.SetMutePref{scope = MUTE_SCOPE_CONVERSATION, group_id, muted}` to record
the preference in the user's private server-side mute set (Mute preference sync).
The preference is per user: muting SHALL NOT change notifications for any other
member and SHALL NOT travel inside the group's encrypted state. It SHALL be
accepted only for a conversation the caller's user is a member of. The server and
the client together enforce this (A muted conversation gets no wake push).

#### Scenario: Mute a busy group
- **WHEN** Alice mutes the "Work" group on her phone
- **THEN** her phone and her other devices stop showing notifications for "Work",
  new "Work" messages still arrive and show as unread, and Bob's notifications for
  "Work" are unchanged

#### Scenario: Unmute
- **WHEN** Alice unmutes "Work"
- **THEN** `Prefs.SetMutePref` removes it from her mute set and notifications for
  "Work" resume on all her devices

#### Scenario: Mute only a conversation you are in
- **WHEN** Alice's device calls `Prefs.SetMutePref` with a `group_id` she is not a
  member of, or an unknown id
- **THEN** the server rejects it with `PERMISSION_DENIED` and her mute set is
  unchanged

### Requirement: Mute the whole app
A user SHALL be able to mute all notifications at once with a single global flag,
and to clear it, by calling `Prefs.SetMutePref{scope = MUTE_SCOPE_GLOBAL, muted}`.
Whole-app mute SHALL suppress the wake push and the in-app notification for every
conversation regardless of the per-conversation mute set, and SHALL override it:
while it is set, an individually unmuted conversation stays silent. The server and
the client enforce this.

#### Scenario: Silence everything
- **WHEN** Alice turns on "mute all"
- **THEN** no conversation raises a push or a notification on any of her devices,
  though messages keep arriving and showing as unread

#### Scenario: Global overrides per-conversation
- **WHEN** "mute all" is on and Alice unmutes a single conversation
- **THEN** that conversation stays silent until "mute all" is turned off

### Requirement: Mute preference sync
The server SHALL keep a private per-user mute set holding the muted conversation
ids and one `muted_all` flag. `Prefs.SetMutePref` SHALL update the caller's user's
set idempotently, and SHALL be accepted only from a device of that user. On every
connect the server SHALL deliver that user's set to each of their devices as one
`ServerFrame.mute_prefs{muted_group_ids, muted_all}` snapshot, and each device
SHALL adopt it as its complete mute set, replacing any it held. The server SHALL
store only the conversation ids and the one flag, never message content or
history, and SHALL expose a user's set to that user's own devices alone. The
server enforces this.

#### Scenario: A second device adopts the mute set
- **WHEN** Alice has muted two conversations and turned off "mute all", and her
  tablet connects
- **THEN** the server sends the tablet one `mute_prefs` snapshot naming those two
  conversation ids with `muted_all = false`, and the tablet silences exactly those

#### Scenario: Only your own set
- **WHEN** Bob's device connects
- **THEN** it never receives Alice's mute set and cannot write to it

#### Scenario: Idempotent mute
- **WHEN** `Prefs.SetMutePref` mutes a conversation already in the user's mute set
- **THEN** the set is unchanged and the call succeeds

### Requirement: A muted conversation gets no wake push
The server SHALL NOT send a content-free wake push to an offline device
(message-delivery: Content-free wake pings) when the owning user set `muted_all`
or when the queued envelope's `group_id` is in that user's muted set; it SHALL
consult the user's mute set before every wake push. Suppressing the push SHALL NOT
drop or delay the message: it stays queued and is delivered on the device's next
`Connect`. The server enforces this.

#### Scenario: Muted group, offline phone
- **WHEN** a message is queued for Alice's offline phone in a group she has muted
- **THEN** the server queues the entry but sends no wake push, and the phone
  receives the message on its next connect

#### Scenario: Unmuted conversation still pings
- **WHEN** a message is queued for Alice's offline phone in a conversation she has
  not muted and "mute all" is off
- **THEN** the server sends the one content-free wake push as usual

#### Scenario: Mute all suppresses every push
- **WHEN** Alice has "mute all" on and a message is queued for her offline phone in
  any conversation
- **THEN** the server sends no wake push and the message waits in the queue
