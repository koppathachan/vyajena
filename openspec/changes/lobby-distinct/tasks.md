## 0. Before starting

- [ ] 0.1 Owner approved the "top" lobby style (Akhil, 2026-11-26): hash tile in an
      accent-tinted square, italic name, "system" tag pill, pinned to the top,
      subtle accent row tint.
- [ ] 0.2 Confirm `lobby-multi-group` (the `ChatKindInfo_Lobby` kind) and
      `app-shell` (the unified chat list) are merged, so the lobby row exists to
      restyle.

## 1. App

- [ ] 1.1 In `app/lib/chat/chat_list_screen.dart`, restyle the
      `ChatKindInfo_Lobby` arm of `_row`: a `#` hash tile on an accent-tinted
      rounded square (replacing the `groups_outlined` `NameAvatar`), the lobby
      title in italics, and a subtle accent `tileColor` on the lobby `ListTile`;
      leave the `ChatKindInfo_Group` and `ChatKindInfo_Dm` arms unchanged; tests
      `app-ui/lobby-row-styling`.
- [ ] 1.2 Add a small "system" pill widget and place it beside the lobby title
      (sharing the title `Row` with the existing mute marker); tests
      `app-ui/lobby-row-styling`.
- [ ] 1.3 Assert the lobby stays pinned first (keep `_arranged` rank 0); add/keep
      a test that the lobby is the first row; tests `app-ui/lobby-row-styling`.

## 2. Tests

- [ ] 2.1 Widget tests: the lobby row shows the hash tile, italic name, "system"
      pill and accent tint and is first; a named group row and a DM row show none
      of these; tests `app-ui/lobby-row-styling`.
