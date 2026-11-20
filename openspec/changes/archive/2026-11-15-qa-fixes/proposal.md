## Why

Two manual QA runs through the real server (`docs/qa/2026-11-13-server-lan.md`,
`docs/qa/2026-11-14-server-lan-run2.md`) passed, but found gaps: a device that joins
later never learns the group name or anyone's display name (F1), a removed user's
devices retry forever with no word (F2), a reset user keeps the old display name
(F3), own messages look like everyone else's (run 1, F4), reconnecting waits out
the backoff after the network returns (F5), the chat says nothing about renames
or member changes (F6), and the server's check misses a pending kernel reboot (F7).
The owner chose to fix them before `qa-verification` (2026-11-14).

## What Changes

- **Own messages:** messages from this user (any of their devices) are
  right-aligned and compact, with no sender label; this device's own messages keep
  the ⏳/✓ status inline. (owner: no label)
- **Late joiners get the name and profiles:** when a commit adds a device, admin
  devices re-send the group name and every device re-sends its own user's profile.
  Re-sends and first syncs change nothing on screen; only real changes show.
- **Reset clears the old profile:** when a user's identity key changes, members
  drop the stored profile for that user. (owner: clear it)
- **Removed devices are told:** the server answers a revoked device's valid
  `Authenticate` (and closes its stream) with `PERMISSION_DENIED`; the core stops
  reconnecting and the app says "This device was removed from the group."
- **Chat notices:** "akhil renamed the group to “Home chat”", "bob joined",
  "akhil removed bob", "bob added a device", "bob removed a device". Automatic
  inactivity removals and re-adds of other devices stay quiet. (owner: all three)
- **Reconnect when the network returns:** the app watches connectivity and tells
  the core to skip its backoff wait. New app dependency: `connectivity_plus`.
- **Server check:** `check.sh` warns when a newer kernel of the running flavour is
  installed but not running.
- Minor: the server logs an invite revoke like it logs an invite create.

No `.proto` changes (`PERMISSION_DENIED` is a gRPC status). No open decision
from the architecture doc is resolved. No invariant changes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `app-ui`: Chat screen, Notices in the chat, Foreground and background.
- `message-content`: Group settings travel encrypted, User-owned profiles.
- `client-runtime`: Connection lifecycle, Events in order.
- `wire-transport`: Session authentication.
- `device-identity`: Device revocation.
- `server-deployment`: Hardening check.

## Impact

- `server/`: devices (`Authenticate`), messaging (stream close), admin (revoke log).
- `client/`: engine (re-send after adds, removed state, reconnect now), store
  (profile clear on key change), events (`Removed`, membership details, `by`).
- `bridge/src/api/`: new event fields, `own_user` on messages, `reconnect_now`;
  frb bindings regenerated.
- `app/`: message tile, notice tiles, connectivity listener, removed state.
- `deploy/server/check.sh`. The server gets the new server (deploy after a backup).
