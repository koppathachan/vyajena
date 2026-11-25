## Why

A real deployed user (a non-admin iPhone, server `v0.4.0` / app `1.4.0+8`) was
stuck unable to send **or** receive in the lobby: every send sat on a silent
hourglass forever (never "failed"), and no new messages arrived. Diagnosis on
`main` (2026-11-25) found this is the **removed-for-inactivity orphan** class,
with an incomplete recovery:

- A backgrounded/suspended device is marked inactive by the server (7 days, or a
  full queue), which **deletes its queued entries** and removes its leaf from the
  group (`group-membership`: Inactive devices are kept out). It is now behind the
  group's epoch.
- The server checks the **strict epoch rule before membership** in the write path
  (`server/src/messaging/write.rs`: `STALE_EPOCH` at the epoch check precedes the
  active-member `POLICY_VIOLATION` check). So a behind/removed device's send is
  rejected with `STALE_EPOCH`, which the core *holds* and retries after catch-up —
  it is never reported "failed". Hence the **silent, indefinite "sending"**.
- The designed recovery (`group-membership`: Automatic re-add) only completes when
  the device's user's other device or an admin device is online and re-adds it. If
  that does not happen, three gaps leave the device a **silent orphan**:
  - **G1 — the device never asks for itself.** The core's startup re-add request
    (`client/src/engine/mod.rs`) fires only when it holds **no** group
    (`store.group().is_none()`). A wedged device still holds a *stale* group, so it
    never self-requests on restart; a force-quit + reopen does not un-wedge it. Its
    only self-request path is "re-add after an unusable Welcome", and an orphan
    receives no Welcome.
  - **G2 — nothing is surfaced.** A device holding a stale group is past the
    group-gate "waiting to be added" screen, so the "waiting"/"re-add limit" UI
    never shows. Its held sends stay a silent hourglass with no terminal state and
    no recovery action.
  - **G3 — removal can be silent.** The "removed for inactivity" notice
    (`RemovedNotice`) that would tell the device (the app shows it as an in-chat
    notice) is not durably delivered: a device offline when it was removed may
    never receive it, so the user sees nothing.

The happy-path auto-re-add is correct and well tested (`client/tests/inactivity.rs`).
This change closes the gaps so a device that falls behind its group **always
recovers or is told to re-sync**, never sits a silent hourglass; and it documents
the admin-driven recovery (remove → re-invite → rejoin) a wiped device needs.

## What Changes

- **A wedged device asks for itself (client, G1).** When the core is holding an own
  message behind the server's named epoch for a group it still has locally, and it
  has caught up on its `Connect` stream yet is still behind (the epoch-advancing
  commit will never arrive, because the device is no longer a current member), the
  core SHALL request re-add (`Keys.RequestReadd`, bounded by the existing per-device
  daily limit) instead of waiting indefinitely. This reuses the server's existing
  re-add path; no new wire field.
- **A terminal, recoverable state instead of a silent hourglass (client + app,
  G2).** When re-add cannot complete (no one able to re-add is online, or the daily
  limit is reached), the core SHALL surface the group as **out of sync** with a
  distinct event, and the app SHALL show a clear in-chat "This chat is out of sync —
  re-sync" banner with a **Re-sync** action (which triggers the re-add request),
  rather than leaving sends an indefinite silent hourglass. A message held behind the
  epoch SHALL NOT remain silently "sending" with no indication once the device is
  known to be stuck.
- **Removal is never silent (server, G3).** The "removed for inactivity" notice
  SHALL be durable: a device that was offline when it was removed SHALL be told it
  was removed (and since when) on its next connect, not only if it happened to be
  online at the moment of removal.
- **Documented admin recovery.** The admin-driven recovery for a wiped or
  unrecoverable device — `Admin.RemoveUser` (frees the username, which
  `Admin.CreateInvite` otherwise refuses), then `Admin.CreateInvite`, then the
  device re-registers on the removed-user reset path and is auto-added back to the
  lobby — SHALL be a first-class, documented flow reachable in the app.
- **Add a member to an existing named group (app).** The admin SHALL be able to add
  an existing user to an existing named group, not only at group creation. Today the
  Members screen is view-only, so a recovered (removed→re-invited) user — or any user
  added after a named group was created — cannot be restored to that named group. The
  lobby and DMs re-add automatically, so this does not block the immediate recovery,
  but it is required for a full restore.
- **Surface invite expiry (app).** An invite is valid only for `invite_ttl` (7 days
  by default); the app SHALL state when a created invite expires, and SHALL tell a
  user whose pasted invite the server rejects as expired that it has expired and a
  fresh one is needed, rather than a generic failure — so the recovery flow stays
  usable when a wiped user does not paste in time.
- **The admin is told when a member goes inactive (server + app).** When the server
  removes a member for inactivity it SHALL notify the admin's connected devices
  (naming the user and since when), so the admin learns a member fell out of the
  group instead of discovering it only from a complaint. The existing rejoin notice
  already tells the admin when such a device returns and needs re-adding.

## Open decisions resolved

- Recovery shape: client self-request on a wedged stale group + a terminal re-sync
  affordance + durable removal notice, reusing the existing `RequestReadd` /
  rejoin machinery (diagnosis 2026-11-25). The admin **single-device lockout**
  (a sole admin device wiped can never re-bootstrap) is raised in design.md as an
  owner decision, tied to `replace-user` Phase 2; it is out of scope here beyond a
  documented "link a second admin device" recommendation.

## Proto files touched

- **None.** This reuses `Keys.RequestReadd` / `ReaddOutcome`, `RemovedNotice`,
  and the rejoin/`JoinNotice` flow as they are. Durable-removal delivery is server
  scheduling + at most a small server-side table (SQLite migration `0010+`, no wire
  field); the out-of-sync signal is a new core `Event` variant surfaced through the
  bridge, not a wire message. It contends for no proto field numbers with the
  in-flight changes (`replace-user`, `mute-notifications`, `video-messages`,
  `delete-messages`, `audio-calls`).

## Capabilities

### Modified Capabilities

- `group-membership`: Automatic re-add gains durable removal notification and a
  wedged device's own re-add request.
- `client-runtime`: Send status gains the rule that a held message is never left a
  silent indefinite "sending" once the device is known stuck.

### Added Capabilities

- `group-membership`: Orphaned-device self-recovery (a device wedged behind its
  group's epoch recovers itself or surfaces re-sync), and Admin is told when a
  member goes inactive.
- `app-ui`: Out-of-sync re-sync affordance, a reachable admin user-recovery flow,
  and adding an existing member to an existing named group.

## Impact

- **Core:** `client/src/engine/mod.rs` (detect wedged-behind-epoch after catch-up;
  drive a self re-add; emit the out-of-sync event), `client/src/engine/outbox.rs`
  (a held-behind-epoch message surfaces re-sync rather than silent sending),
  `client/src/events.rs` (new `OutOfSync` / re-sync event), `bridge/` + `app/lib/`
  bindings. `client/tests/inactivity.rs` (an orphan with no re-adder self-requests
  and surfaces re-sync; recovers on Re-sync / admin online; never a silent
  indefinite sending).
- **Server:** durable removal-notice delivery on reconnect (`server/src/messaging/`
  + a small record; migration `0010+`); `server/tests/` (a device offline when
  removed is told on its next connect).
- **App:** an out-of-sync banner + Re-sync action in the chat; the admin
  user-recovery (remove → re-invite) flow surfaced and documented.
- **Ops:** no new credentials; one SQLite migration (`0010+`); no relay change.
