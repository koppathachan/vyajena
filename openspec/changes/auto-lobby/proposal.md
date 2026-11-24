## Why

On a fresh server the admin's first device lands on a "Create the group" screen and
must tap a button before the lobby exists (`app/lib/chat/group_gate.dart` →
`CreateGroupScreen`). The owner expects the lobby to be there from the start: the
admin should open the app and already be in the lobby, ready to invite people. This
also sharpens the self-hostable first run (`self-hostable`): a new operator joins
from the bootstrap invite and is immediately in a working lobby.

Hard constraint (CLAUDE.md invariants): the server cannot mint an MLS group — only
devices create MLS groups and keys never leave the core. So the lobby cannot be
"created by the server". The fix is to have the **admin's first device** create the
lobby automatically on first open, instead of waiting for a human to tap. The
server already holds at most one system group and rejects a second
`Admin.CreateGroup` with `ALREADY_EXISTS` (group-membership: Group creation), so an
automatic call is safe even from a linked admin device that has not synced yet.

## What Changes

- **The admin's first device creates the lobby automatically (app-ui: The lobby
  appears without a manual create step).** When the device is the admin's and holds
  no lobby, the app calls `Admin.CreateGroup` itself and lands in the lobby — no
  "Create the group" button.
- **A linked admin device handles `ALREADY_EXISTS` benignly.** Its automatic create
  may race an existing lobby; it then simply waits to be synced in, with no error.
- **Manual creation stays for additional named groups** (`Admin.CreateNamedGroup`),
  which the admin still makes deliberately.
- **Scope: the admin's first device only.** Later members' devices are unchanged —
  they wait to be added (app-ui: Join from an invite).

No server change is required (the behaviour rides on the existing `Admin.CreateGroup`
and its `ALREADY_EXISTS` guard). Optionally, the server could record a
"default-lobby intended" marker at bootstrap, but it is not needed: the device-side
auto-create is sufficient and keeps MLS group creation on the device.

## Proto files touched

- **None.** Uses the existing `Admin.CreateGroup`.

## Capabilities

### New Capabilities

<!-- None. -->

### Modified Capabilities

- `app-ui`: new requirement "The lobby appears without a manual create step".

## Impact

- **App (`app/lib/chat/group_gate.dart`):** when `admin && !inGroup`, call
  `core.createGroup()` automatically (with a brief setting-up state) instead of
  showing `CreateGroupScreen`; treat `ALREADY_EXISTS` as benign and fall through to
  the waiting/synced path; keep a visible retry only for a genuine failure.
- **App (`app/lib/admin/create_group_screen.dart`):** no longer the lobby's first
  step; it (or its logic) stays only for named-group creation if used there.
- **Core / server / proto:** none.
- **Relation to other changes:** builds on `lobby-multi-group` (Group creation, the
  `ALREADY_EXISTS` guard) and completes the `self-hostable` first-run UX; neither is
  edited here.
