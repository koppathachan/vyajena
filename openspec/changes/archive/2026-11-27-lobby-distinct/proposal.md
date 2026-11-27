## Why

The chat list already puts the lobby first and labels it "lobby"
(`lobby-multi-group`, `app-shell`), but it looks like any other group row — a
plain avatar and name. The lobby is special: it is the single, undeletable,
unrenameable, all-members system group every user is in. New users land there, and
it is the one chat nobody chose and nobody can leave. It should *read* as the
system space, not as one group among many.

The owner approved a distinct treatment (2026-11-26): a `#` hash tile in an
accent-tinted square, the name in italics, a small "system" tag, pinned to the top,
and a subtle accent tint on the row — so the lobby is recognisable at a glance and
visually separated from the admin's named groups and from DMs.

This is **app presentation only** — no protocol, proto, server, core or wire
change. It restyles one row kind in the existing chat list.

## What Changes

- **Hash tile avatar.** The lobby row's leading avatar SHALL be a `#` hash glyph
  on an accent-tinted rounded square (replacing today's `groups_outlined`
  `NameAvatar`), marking it as the system space.
- **Italic name.** The lobby name ("lobby") SHALL render in italics, distinct from
  the upright names of groups and DMs.
- **"system" tag pill.** A small "system" pill SHALL sit next to the lobby name.
- **Pinned to the top.** The lobby SHALL remain pinned first in the list (as it is
  today); this change states it as a requirement of the distinct treatment, not a
  new sort.
- **Subtle accent row tint.** The lobby row SHALL carry a subtle accent-tinted
  background, setting it apart from the untinted group and DM rows.
- **Lobby only.** The treatment SHALL apply only to the lobby
  (`ChatKindInfo_Lobby`), never to admin-created named groups
  (`ChatKindInfo_Group`) or DMs (`ChatKindInfo_Dm`).
- **Out of scope:** the lobby conversation screen; the lobby name string or its
  immutability (owned by `lobby-multi-group`); theming beyond the existing
  Material 3 color scheme; any server/proto/core change.

## Open decisions resolved

- Owner (Akhil, 2026-11-26): approved the "top" lobby style — hash tile in an
  accent-tinted square, italic name, a "system" tag pill, pinned to the top, and a
  subtle accent row tint.

## Proto files touched

- **None.** No `.proto`, `protocol/`, server or core change. This change only
  restyles the lobby row in the Dart chat list; it contends for no proto field
  numbers.

## Capabilities

### Modified Capabilities

- `app-ui`: the chat list gains a distinct visual treatment for the lobby row.

## Impact

- **App (`app/`):** `app/lib/chat/chat_list_screen.dart` — the `_row` builder's
  `ChatKindInfo_Lobby` arm (the leading avatar, the title styling, the row tint)
  and a small "system" pill widget; the lobby pin already comes from `_arranged`
  (rank 0) and is unchanged. The `lobbyName` constant (`app/lib/chat/chat_model.dart`)
  is reused as-is.
- **No bridge change:** the row is built from the `ChatSummary` / `ChatKindInfo`
  the core already provides; no new field is needed.
- **Tests:** `app/` widget tests — the lobby row shows the hash tile, italic name,
  "system" pill and tint; a named group and a DM row show none of these.
- **Depends on:** `lobby-multi-group` (the `ChatKindInfo_Lobby` kind) and
  `app-shell` (the unified list); it restyles what they render.
