## Context

The `dms` change (Complete, not yet archived) made the home screen a chat list,
but only when the user already has at least one DM; otherwise the home is still
the group's chat, with a `forum` "Chats" button in its app bar that pushes the
list. DMs can only be *started* from the members list's "Message" action, and the
admin can only create the group on first start (`GroupGate` → `CreateGroupScreen`).

The core already exposes everything this needs over `flutter_rust_bridge`:
`CoreService.chats()`, `openDm(user)`, `createGroup()`, `isAdmin()`,
`dmsAvailable()`, `members()`. This change is UI only (`app/lib/`); no proto,
core, server, or relay change, and no invariant touched.

Relevant current code:
- `app/lib/chat/group_gate.dart` picks the home: `ChatListScreen` when `_hasDms`,
  else `ChatScreen` (the group).
- `app/lib/chat/chat_list_screen.dart` lists the chats and opens each (the group's
  `ChatScreen`, or a DM via `showDm`).
- `app/lib/chat/chat_screen.dart` carries the `forum` "Chats" button, shown only
  when `_model.isMain && _model.hasDms && !Navigator.canPop()`.
- `app/lib/members/members_screen.dart` has the "Message" → `openDm` path.

## Goals / Non-Goals

**Goals:**
- The chat list is always the home once this device is in the group.
- A `+` FAB starts a new conversation: New DM (everyone, gated by `dmsAvailable`),
  New group (admin only, `isAdmin`).
- The members-list / profile "Message" path keeps working unchanged.

**Non-Goals:**
- No change to how DMs or groups work in the core, server, or wire protocol.
- No multi-group model: the MVP is one admin-run group (see D4).
- No change to the chat-list row contents (unread marks, mentions, closed DMs)
  beyond what the `dms` change already ships.

## Decisions

### D1: The chat list is always the home
`GroupGate` routes to `ChatListScreen` whenever `epoch() != null` (in the group),
dropping the `_hasDms` branch and the per-event `chats()` probe it needed. The
group's `ChatScreen` is always pushed over the list, so the back button returns to
the list. Alternative (keep the conditional): rejected — it splits the home into
two layouts, makes the `+` affordance's home ambiguous, and keeps the dead `forum`
button alive.

### D2: Remove the `forum` "Chats" button
With the list always the home, the group's `ChatScreen` is always a pushed route,
so `!Navigator.canPop()` is always false and the `forum` button never showed. Its
whole block (and the `dmUnread` badge it carried) is removed from `ChatScreen`;
`ChatListScreen` already imports `ChatScreen`, not the reverse, so the
`chat_list_screen.dart` import in `chat_screen.dart` goes too. Returning to the
list shows each DM's unread mark, which replaces the in-group count.

### D3: The `+` FAB and the member picker
`ChatListScreen` gains a `+` `FloatingActionButton` whose availability is decided
from two booleans it already has or loads: `dmsAvailable()` (loaded beside
`chats()`) and the `admin` flag passed in from `GroupGate`.
- The `+` opens a small bottom-sheet menu (`showModalBottomSheet`) listing the
  available actions: New DM when `dmsAvailable`, New group when `admin`. A single
  available action still appears as a one-item menu, so the affordance is
  consistent and the "opens the + menu" scenario holds.
- Neither available → no FAB (a non-admin on a protocol-v1 server).

New DM opens a member picker (`showModalBottomSheet` listing the other members
from `members()`, deduped by user, excluding this device's own user), then calls
`openDm(user)` and shows the DM with the existing `showDm` helper — the same core
path the members list uses, so a DM that already exists is reused by the core.

New group calls `createGroup()`. The result refreshes the list.

Alternative (a `SpeedDial`/overlay FAB package): rejected — the app uses stock
Material widgets only (app-ui: Light Material 3 look); a bottom sheet is stock.

### D4: "New group" in a one-group MVP
Groups are admin-only to create (owner decision, recorded in the proposal). In the
current one-group MVP the admin's group already exists by the time the chat list
is the home (the gate shows `CreateGroupScreen` first otherwise), so New group is
primarily future-facing and simply calls `createGroup()`. It is gated by
`isAdmin()` so only the admin ever sees it; non-admin group RPCs are rejected by
the server regardless (app-ui: Admin screens). See Open Questions.

## Risks / Trade-offs

- [The list always being the home adds one navigation level for users with no DMs,
  who previously saw the group chat directly.] → The group is the first row and one
  tap away; this is the point of the change (a consistent home with a `+`).
- [`create-affordance` MODIFIES the **Chat list** requirement, which currently
  lives in the unarchived `dms` change, not in `openspec/specs/app-ui/`.] → The
  `dms` change must be archived before `create-affordance` so the base requirement
  is in the main spec at archive time; the tracker coordinates archive order. The
  change validates `--strict` on its own regardless.
- [New group may be a no-op or surprising if the group already exists (D4).] →
  Gated to the admin only and left for the owner to confirm (Open Questions).

## Migration Plan

Pure UI; ships with the app build. No data migration, no server change. Rollback
is reverting the commit.

## Open Questions

- **New group semantics in a one-group MVP (D4):** should the admin's `+` "New
  group" be hidden once the group exists, show a confirmation, or stay as a plain
  `createGroup()` call? Left as the brief specified (`isAdmin()` → `createGroup()`),
  flagged for the owner.
