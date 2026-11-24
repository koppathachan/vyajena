## Why

The admin's only way to "rename" or re-issue a person is to remove them and invite
them again (`replace-user` Phase 1 — there is no in-place rename). On the live
server (2026-11-24) the owner did exactly this and it broke, in two ways that share
one root cause: **user removal is server-side only.** The removed device is never
told it was removed, the core never clears its own state, and the core has no
reset/unregister API at all.

- The removed person's device still showed its old group (`world`) and DM
  (`vyajan`). Removal revokes the device's auth server-side but sends the device no
  durable signal; a group clears only if the device was online to process its own
  MLS eviction commit, and the DM-closed notice is queued only for the peer
  (`server/src/dms/mod.rs:626`), never the removed user's own devices.
- The fresh invite could not onboard **on that same device**. `Client::register`
  returns `AlreadyRegistered` whenever a local account exists
  (`client/src/client.rs:391`) and the core has no reset path, so a removed device
  is a permanent dead end — even though the server would accept the re-register (it
  has a removed-user reset path at `server/src/devices/mod.rs:423`).

This is tracker finding **F2** plus a second half (no way back). It blocks the whole
remove + re-invite workflow and therefore `replace-user` Phase 1, which is built on
it. Root-cause detail: shared note `remove-reinvite-bug-diagnosis-2026-11-24.md`.

## What Changes

This one change is both the bug fix and `replace-user` Phase 1. The fix (the core
of the change) must land for the feature to work; the admin "Replace user" screen is
the last, separable slice and can be deferred if the owner wants the fix to ship
first.

- **The core can reset itself.** A new core API `reset()` (bridge-exposed) returns
  the device to a clean "no account" state: it drops the local account, groups, DMs,
  message history, pending jobs and the device's identity/auth/MLS/storage keys from
  the OS key store. It runs only on an explicit caller request after the core has
  emitted `Removed`, never silently.
- **`register()` is no longer a dead end.** After `reset()` the local account is
  gone, so a fresh invite onboards normally against the server's existing removed-user
  reset path. `register()` itself is unchanged on the wire.
- **The server tells the removed device in-band, not only revokes it.** On
  `Admin.RemoveUser` the server delivers an account-level "you were removed" frame on
  each of the removed user's own **connected** streams before the revoke tears them
  down, so an online device learns it cleanly instead of inferring removal only from a
  later `PERMISSION_DENIED`. A device that was offline during removal still learns on
  its next connect, because `Devices.Authenticate` answers `PERMISSION_DENIED` and the
  core already treats that as `Removed` — the client-side reset is what unblocks both
  cases.
- **A closed DM is cleared on both sides.** `close_dms_of` queues `DmClosed` only for
  the peer today. The removed user's own connected devices get it too (best-effort on
  the live stream), so an online removed device clears the DM from its list before the
  user resets; after `reset()` the DM is gone regardless.
- **App — removed device:** a terminal "You were removed from this server" screen with
  a single "Start over" action that calls `reset()` and returns to the invite screen.
  History stays readable until the user confirms the erase.
- **App — admin (deferrable slice):** a "Replace user" flow on the admin's devices
  that removes an existing user and issues a fresh invite (same or new username) in
  one guided screen, now that removal actually propagates.
- **Out of scope:** Phase 2 co-signed succession (carrying identity across a rename
  without a wipe); in-place rename with no removal; selective reset that keeps some
  chats; blocking/muting.

Details and decisions: `design.md`.

## Open decisions resolved

- Re-onboard on the same physical device: yes, via an explicit user-confirmed
  `reset()` that wipes local state (owner signalled "reset clears profile" on
  2026-11-15; confirm for this change).
- Auto-wipe vs. user-driven reset: **user-driven** — no silent loss of a device's
  local history (D2). (Confirm with owner.)
- New username vs. reused username on replace: both; the admin chooses (unchanged from
  `group-membership`: Removing a user).

## Proto files touched

- `proto/vyajena/v1/messaging.proto`: one new `ServerFrame.frame` variant for the
  account-level removal notice. **The existing `RemovedNotice` message is NOT this** —
  it is the per-group inactivity re-add notice (`messaging.proto:247`), so the new
  frame needs a distinct name and field number. Field numbers are left TBD here and
  MUST be reconciled at implementation with the in-flight `remove-closed-dm` (which
  also adds a per-user, connect-delivered marker and may share this mechanism),
  `mute-notifications` and `notification-reliability`, which all add connect/marker
  machinery. See design D1/D6.

## Invariants

No cryptographic invariant changes. "Accepted means durable; nothing is dropped
silently" is reinforced: a removal now delivers an in-band notice to the removed
user's own connected devices, not only to everyone else. The reworded
`group-membership: Removing a user`, `device-identity: Signup with an invite` and
`client-runtime: Connection lifecycle` requirements need owner approval before apply.
