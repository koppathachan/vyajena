## Why

Message text with a URL in it is plain, unclickable text today: `message_tile.dart`
renders a `Text` (or a `Text.rich` for mentions) and the user must copy a link by
hand. The owner asked (2026-11-20) to make URLs tappable, opening in the external
browser. The architecture doc already lists "no link previews" as deliberate; this
change keeps that — it makes links tappable, it does not fetch anything.

This is a display-only app change: no wire, no core, no new content. The text
already arrives decrypted in Dart to be shown; the app only draws the URLs in it
as tappable spans and hands a tapped URL to the OS.

## What Changes

- **URLs in message text and image captions become tappable** (owner,
  2026-11-20).
  - **What counts as a link:** only explicit `http://` and `https://` URLs (owner,
    2026-11-20). No bare `example.com`, no `mailto:`/`tel:`, no autolinking of
    `www.` — fewest false positives, most predictable in an E2EE chat where anyone
    may paste anything.
  - **Tapping shows a confirm sheet with the full destination URL**, and Open /
    Copy / Cancel (owner, 2026-11-20). Only on Open does the app hand the URL to
    the OS browser (`url_launcher`, `LaunchMode.externalApplication`). This is the
    anti-spoofing speed bump the app's security posture wants.
  - **Links render in both message text and image captions** (owner, 2026-11-20),
    reusing the same span builder that draws mentions. Where a detected link range
    overlaps a mention span, the mention wins (mentions are structured; links are
    heuristic).
  - **Same behaviour in the lobby, named groups and DMs** (owner, 2026-11-20).
- **No link previews, still** (message-content: Text only, unchanged intent): the
  app fetches no title, image or metadata for a URL; it opens no connection
  itself.
- **Out of scope:** in-app browsing, link previews/unfurling, `mailto:`/`tel:`,
  autolinking bare domains, marking links in the composer as the user types.
- Details: `design.md`.

## Open decisions resolved (owner, 2026-11-20)

- **Schemes:** `http`/`https` only.
- **Open behaviour:** a confirm sheet showing the full URL, then an external open.
- **Scope:** message text and image captions; the mention wins on an overlap.
- **Per-chat:** identical in the lobby, named groups and DMs.

## Proto files touched

- None. No `.proto`, `protocol/`, server or core change; the app draws and opens
  links from text the core already hands it.

## Capabilities

### Modified Capabilities

- `app-ui`: URLs in message text and captions are tappable and open externally
  after a confirm.
- `message-content`: the "Text only / no previews" rule is restated to allow
  display-only linkifying while still forbidding fetched previews.

## Impact

- **App (`app/`):** a small URL detector reused by the text and caption span
  builders (`message_tile.dart`, `mentions.dart`); a confirm sheet; the
  `url_launcher` dependency.
- **No bridge, core, server or proto change.**
- **Depends on:** nothing new. Lands independently of `lobby-multi-group` and
  `dms` (it only draws text those changes already show).
