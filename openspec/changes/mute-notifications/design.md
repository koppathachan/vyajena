## Context

The wake push is sent by the server (message-delivery: Content-free wake pings):
it carries no content, only "New message", and it is the only notification signal
the server controls. Because the app is E2EE, the server cannot read a message to
decide whether it matters to the recipient. So "mute" can only be "server, do not
send the wake push for this conversation (or at all) to this user's devices", plus
the app suppressing its own local notification and badge.

Mute is a *personal preference*: Alice muting a group must not change anything for
Bob, and must not travel inside the group's encrypted state. It therefore cannot
ride `GroupSettings` (group-shared) or any per-message channel. There is no
self-only channel among a user's devices — which is exactly the gap
`remove-closed-dm` closed with a private per-user server-side marker synced on
connect. Mute reuses that rail.

## Decisions

### D1. A private per-user mute set on the server (the `remove-closed-dm` rail)

- `Prefs.SetMutePref{scope, group_id, muted}` writes the caller's user's private
  mute set, idempotently:
  - `scope = MUTE_SCOPE_CONVERSATION` with `group_id` adds (`muted = true`) or
    removes (`muted = false`) that conversation id from the set. The server accepts
    `group_id` only when it is a conversation the caller's user is a member of (a DM
    they belong to, the lobby, or a named group they are in); any other id —
    unknown, or a conversation the caller is not in — is `PERMISSION_DENIED`, so ids
    of others' conversations are not probed.
  - `scope = MUTE_SCOPE_GLOBAL` sets (`muted = true`) or clears (`muted = false`)
    the user's `muted_all` flag; `group_id` is ignored.
- On every connect the server sends the caller's mute set as one
  `ServerFrame.mute_prefs{muted_group_ids, muted_all}` snapshot, after the close
  notices and the removed-conversation set. It is a **snapshot, not a queue**: no
  ack, and a device linked later gets the current set. The server stores only the
  conversation ids (16 bytes each) and the one boolean; no content or history, so
  the no-durable-history invariant holds.
- Only that user's devices write (through the authenticated RPC, keyed by the
  caller's username) or read (connect frames go to the connecting device) the set.
- **Why a single snapshot frame, not one-per-entry like `conversation_removed`:**
  mute carries a global flag as well as the id set, and the set shrinks as well as
  grows (unmute), so a self-contained snapshot the device adopts wholesale is
  simpler and idempotent than a stream of add entries. `conversation_removed` only
  ever grows, so a per-entry stream suited it.

### D2. The server enforces the mute at push time; the app enforces it locally

- **Server (the part that actually silences the phone).** Before enqueuing a wake
  push for an offline device (message-delivery: Content-free wake pings), the
  server looks up the owning user's mute set: if `muted_all`, or the envelope's
  `group_id` is in `muted_group_ids`, it skips the push. The message is still
  queued and delivered on the next `Connect`; only the push is suppressed.
- **App (the in-app half).** When a message arrives on an open stream for a muted
  conversation, the app does not raise a local notification or sound for it and
  does not bump the app notification badge; the chat still shows unread. The app
  learns the mute set from `mute_prefs` and from its own `SetMutePref` calls.
- The core does not gate the server's push (it cannot; the device is offline when
  the push matters). The two halves cover the two states: offline → server skips
  the push; foreground/connected → the app suppresses its own notification.

### D3. On/off only for the MVP

The set stores presence (muted or not) and one global boolean — no expiry
timestamps. Timed mute (WhatsApp-style 1h/8h/until-I-unmute, a server-stored
`muted_until`) is a later nicety: it needs an expiry column, a clock at push
time, and UI. Noted, not built.

### D4. No @mention exception for the MVP

Letting a muted *group* still ping you when a message @mentions you would require
the server to know the message mentions you — but the message is E2EE, so the
server cannot read it. The only way would be the sender's device flagging the
mentioned users in the routing envelope, which leaks mention metadata (who was
mentioned, by whom) to the server. That is a real privacy regression for a
convenience feature, so it is deferred; mute is unconditional for the MVP.

### D5. Whole-app mute overrides per-conversation

`muted_all` suppresses every conversation's push and notification regardless of
the per-conversation set. Unmuting a single conversation while `muted_all` is set
has no effect until `muted_all` is cleared; the app shows the global state so this
is not surprising.

```mermaid
sequenceDiagram
    participant P as Alice phone
    participant S as server
    participant T as Alice tablet
    P->>P: mute "Work" group
    P->>S: Prefs.SetMutePref(CONVERSATION, work_gid, muted=true)
    S->>S: add work_gid to alice's mute set
    Note over S: Bob later sends to "Work"; Alice's phone is offline
    S->>S: queue entry for alice's phone; mute set has work_gid -> skip wake push
    Note over T: on next connect
    S->>T: ServerFrame.mute_prefs({work_gid}, muted_all=false)
    T->>T: adopt mute set; no local notification for "Work"
```

## Risks / Trade-offs

- **[The server learns which conversations a user muted.]** It already routes them
  and decides their pushes, so this is incremental metadata, not new exposure of
  content; it stays private to that user's own devices and reaches no other user.
- **[A stale mute set on a long-disconnected device]** → the snapshot on connect
  replaces the local set wholesale, so a device catches up on every mute/unmute it
  missed in one frame.
- **[An old server returns `UNIMPLEMENTED` for `SetMutePref`]** → the app surfaces
  it and leaves the conversation unmuted (fail-open: the user keeps getting pinged,
  which is safe, not silent).
- **[Field-number contention]** → `mute_prefs = 12` follows
  `conversation_removed = 11`; `prefs.proto` is a new file with its own tag space.
  `buf breaking` gates any clash at implementation.

## Migration Plan

Spec-only here (builds serialize on the shared external-disk cargo target; not
implemented in this change). When built: add `prefs.proto` and `mute_prefs = 12`,
regenerate; add the mute-prefs migration — numbered `0010_mute_prefs.sql` on the
server and `0011_mute_prefs.sql` on the client, since `0009`/`0010` were already
taken on this branch (`0009_catchup.sql` on the server,
`0010_identity_retrust.sql` on the client) by the time this change was built, so
the proposal's `0009` is the next free slot, not that literal number; implement
`Prefs.SetMutePref`,
the connect-time `mute_prefs` snapshot, and the push-time mute check; add the
core/bridge surface and the app controls. Server upgrades before the app.
