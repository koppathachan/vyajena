## Why

Today the main group chat is the home screen and DMs are reachable only behind a
`forum` icon that appears once a DM already exists (change `dms`); there is no way
in the app to *start* a DM except the "Message" action buried in the members list,
and no in-app way for the admin to create the group once past first start. The
home screen should be a chat list, and starting a new conversation should be a
first-class `+` action, so the group and every DM sit side by side and a new chat
is one tap away.

## What Changes

- The home screen is **always** the chat list (the group first, then the DMs),
  for every device in the group, instead of only when the user already has a DM.
  The group's chat opens on top of the list.
- A `+` **FloatingActionButton** on the chat list starts a new conversation:
  - **New DM** (every device, only when the Pi offers DMs via `dmsAvailable()`):
    pick another member, then open the DM through the core's `openDm` and show it
    at once — the same core path the members-list "Message" action already uses.
  - **New group** (admin devices only, via `isAdmin()`): create the group through
    the core's `createGroup`. Non-admin devices never see it.
- The group chat's `forum` "Chats" app-bar button is removed: with the list always
  the home, the group chat is always pushed over it, so the button was already dead
  (it only showed when the list could not be popped to).
- The members-list / profile "Message" path is unchanged and keeps working.

No wire-protocol, proto, or core change: the core already exposes `chats`,
`openDm`, `createGroup`, `isAdmin` and `dmsAvailable`. This is a UI-only change.

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
- `app-ui`: the **Chat list** requirement changes so the app always opens on the
  chat list (removing the "open on the group's chat when there is no DM" and the
  "a DM that arrives while the group is open offers the list from the group's app
  bar" behaviour). A new **Create affordance** requirement adds the `+` FAB with
  its "New DM" and admin-only "New group" actions.

## Impact

- UI only, under `app/lib/`:
  - `app/lib/chat/group_gate.dart` — always route to the chat list once in the group.
  - `app/lib/chat/chat_list_screen.dart` — add the `+` FAB and its member picker.
  - `app/lib/chat/chat_screen.dart` — remove the now-dead `forum` "Chats" button.
- New widget tests in `app/test/` tagged `// spec: app-ui/<slug>`.
- Open decisions / owner decisions this records: the home is always the chat list;
  groups are admin-only to create (`+` "New group" gated by `isAdmin()`).
- No `.proto` files touched; no core, Pi, or relay change.
