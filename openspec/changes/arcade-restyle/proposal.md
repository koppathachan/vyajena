## Why

The owner wants a bold retro "Arcade Motion" look for the Flutter app: square
corners, thick borders, hard offset shadows, a pixel font for headings and a
monospace body, stepped motion. It is a visual change only: layout, content,
behaviour, the core and the wire are untouched.

## What Changes

- Replace the `Light Material 3 look` requirement (`ColorScheme.fromSeed`, stock
  widgets, no custom fonts) with an `Arcade look` requirement: one explicit token
  set, light and dark following the system; radius 0; 2px borders; hard shadows
  (cards 6px, buttons 3px, chat bubbles 2px); one bundled font, Press Start 2P
  (SIL OFL), for headings and labels; the platform monospace for body text.
- Add `Arcade motion`: stepped curves only, a reduced-motion switch, no motion in
  the message list, text scale capped.
- No new pub dependency, no image assets, no proto, core or bridge change.

## Impact

- `app/lib/app/theme.dart` and a new `app/lib/app/arcade/`; small edits in chat,
  shell, list and form widgets; `app/pubspec.yaml` (font asset only);
  `app/assets/fonts/` (Press Start 2P + its licence).
- `app/test/app/look_test.dart` is rewritten; other widget tests that match
  exact label text are updated where a label becomes uppercase.
