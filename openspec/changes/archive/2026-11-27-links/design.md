## Context

- **`message_tile.dart` renders text in two ways today:** a plain `Text(text)`
  when a message has no mentions, and a `Text.rich(mentionSpans(...))` when it
  does. Image captions go through the same `_withMentions` helper.
- **`mentions.dart` builds the mention spans** from `message.mentions`
  (UTF-16 ranges) and draws the reader's label for each target, highlighting
  mentions of the reader.
- **The app already shows decrypted text** (app-ui: UI goes through the core);
  Dart opens no socket and holds no key. `url_launcher` hands a URL to the OS and
  opens no connection in Dart.

## Goals / Non-Goals

**Goals**
- Make `http`/`https` URLs in text and captions tappable, opening externally after
  a confirm.
- Keep "no link previews": fetch nothing.
- Reuse the mention span machinery so links and mentions coexist.

**Non-goals**
- Autolinking bare domains, `mailto:`/`tel:`, in-app browsing, previews/unfurling,
  composer-time link styling.

## Decisions

### D1. Detect only `http`/`https`, with a conservative matcher

- **A URL is a run beginning `http://` or `https://`** followed by non-space URL
  characters, with trailing punctuation (`. , ) ] } ! ? : ;` and a closing quote)
  trimmed off the match so "see https://a.com." does not swallow the full stop.
- **No scheme-less matching.** "example.com", "www.x.io", emails and phone numbers
  are left as plain text (owner, 2026-11-20).
- **Offsets are UTF-16 ranges**, like `MentionSpan`, so the detector and the
  mention spans share one coordinate space.
- **Why conservative:** a false link in an E2EE chat is a phishing surface; the
  owner chose the tightest useful rule.

### D2. Links and mentions in one span pass; mention wins on overlap

- **One builder produces the `InlineSpan` list** for a string: it takes the
  mention spans (already structured) and the detected link ranges, and walks the
  string once. A range that overlaps a mention is dropped for the link layer — the
  mention keeps that text (mentions are explicit sender intent; a link is a
  guess).
- **Non-overlapping link ranges become tappable spans** (a `TextSpan` with a
  `TapGestureRecognizer`), drawn in the theme's link colour and underlined.
- **The same builder serves text and captions**, so both get links (owner,
  2026-11-20).

### D3. Tap opens a confirm sheet, then an external launch

- **A tapped link opens a modal sheet** showing the full destination URL (so an
  obfuscated or look-alike URL is visible before anything opens), with **Open**,
  **Copy** and **Cancel**.
- **Open calls `url_launcher`** with `LaunchMode.externalApplication`, handing the
  URL to the OS browser. The app opens no connection and stores nothing.
- **Copy** puts the URL on the clipboard. **Cancel** dismisses.
- **If no handler exists** (launch returns false), the sheet shows "Couldn't open
  this link" and offers Copy.

### D4. The same everywhere

- **Lobby, named groups and DMs use the identical path** (owner, 2026-11-20):
  there is one tile and one span builder; nothing branches on the chat kind.

## Risks / Trade-offs

- **A hostile link is still a link.** The confirm sheet shows the real URL but the
  app cannot judge where it leads; this matches every chat app and is why the
  sheet shows the full destination rather than opening silently.
- **Unicode / IDN look-alikes:** the sheet shows the URL as written. Punycode
  display hardening is out of scope; noted as an open question.
- **No preview means less context**, deliberately (the arch doc's "no link
  previews").

## Open Questions (owner)

1. **Punycode / IDN:** show a warning on mixed-script or `xn--` hosts in the
   confirm sheet, in a later change?
2. **Long URLs in the sheet:** middle-ellipsise, or show in full and scroll?
