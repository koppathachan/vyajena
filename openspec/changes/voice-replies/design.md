## Context

Two coupled gaps in replies-to/with voice messages, both found in the 2026-11-24
self-hosting test:

- **(A) You cannot reply *with* a voice note.** `reply_to` lives only inside the
  `Text` body (`proto/vyajena/v1/content.proto`), so only a text message can carry
  a reply. The composer enforces the same shape: `_micShown`
  (`app/lib/chat/composer.dart`) requires `widget.reply == null`, so swiping to
  reply hides the mic and the right-hand button becomes Send. A voice note can
  never answer a message.
- **(B) Quoting a voice message shows "needs a newer version".** When the core
  builds the `ReplyTo` for a voice original (`client/src/client.rs::reply_ref`), the
  quote is `String::new()` — a voice message has no text to quote (a captionless
  picture is empty too). The receiving app renders the quote with
  `reply.quote.isEmpty ? unsupportedText : reply.quote`
  (`app/lib/chat/message_tile.dart`), so an empty quote becomes the "This message
  needs a newer version of the app to be shown." placeholder. The composer's reply
  bar already shows "Voice message" because it derives the label from the local
  original (`chat_screen.dart::_startReply`, `voiceQuote`), but that label never
  reaches the wire, so a receiver — or the sender's own bubble, which renders from
  the wire `ReplyTo` — has nothing.

Invariant check (CLAUDE.md): quotes are **client-side content** carried inside the
encrypted `Content`; the server stores no durable history and never sees a quote.
Both fixes stay entirely inside `Content` (the MLS application plaintext). The
server, relay and wire envelope are untouched. Keys never leave the core.

## Decisions

### D1: Carry a reply-to on the `Audio` body (gap A)

`reply_to` stays where it is for text (unchanged, per the task). For audio, add a
`ReplyTo reply_to = 6;` to the `Audio` message (field 6 is the next free field;
1–5 are the existing audio fields). This mirrors `Text.reply_to` exactly, so the
core, encode/decode and UI reuse the same `ReplyTo` plumbing. An old client that
predates the field ignores it and still plays the voice message (proto3 unknown
field), so a voice reply degrades to a plain voice message on an old app — never
to the "needs a newer version" placeholder, because the body is still `Audio`.

Rejected alternative — promote `reply_to` to `Content` level so any body carries a
reply uniformly: it would change the wire position of *text* replies (breaking
"keep text replies unchanged") and is a larger migration for no MVP gain. Images
can gain `reply_to` later the same way audio does here.

### D2: Make the quote self-describing so a receiver without the original renders a proper preview (gap B)

The quote string alone cannot describe a voice original (there is no text), and
the receiver may not hold the original message. So `ReplyTo` gains two small,
additive fields, both inside the encrypted `Content`:

```proto
message ReplyTo {
  DeviceRef sender = 1;
  bytes msg_id = 2;
  string quote = 3;                 // text excerpt; empty for a voice/captionless original
  OriginalKind original_kind = 4;   // what the original was
  uint32 original_duration_ms = 5;  // the voice original's length, 0 otherwise
}

enum OriginalKind {
  ORIGINAL_KIND_UNSPECIFIED = 0;  // unknown / old sender -> treat as a text quote
  ORIGINAL_KIND_TEXT = 1;
  ORIGINAL_KIND_IMAGE = 2;
  ORIGINAL_KIND_AUDIO = 3;
}
```

- The core (`reply_ref`) sets `original_kind` from the original's decoded body and
  `original_duration_ms` from an audio original's `duration_ms` (0 otherwise). The
  text `quote` stays as today (text excerpt, or a picture's caption, or empty).
- The receiving app renders the quote block from `original_kind`, not from
  `quote.isEmpty`: `AUDIO` → a mic glyph plus the duration (e.g. "🎙 0:07") and the
  "Voice message" label; `IMAGE` with an empty quote → a picture glyph and
  "Picture"; `TEXT`/`UNSPECIFIED` → the text quote as today. The `quote.isEmpty →
  unsupportedText` rule is removed; the placeholder is reserved for a genuinely
  unknown body, which `original_kind` now distinguishes.
- Back-compat: an old sender leaves `original_kind = UNSPECIFIED (0)` and a
  non-empty text quote, so old text replies render unchanged. An old sender's voice
  reply quote is still empty with kind 0 — the one residual case — and the app
  shows a neutral "Voice message" label for an empty quote with kind `UNSPECIFIED`
  *when the original is audio in the local store*, else the neutral label; it never
  shows the "needs a newer version" text for a reply quote again.

Why carry duration on the wire rather than read it from the local original: the
receiver often does not have the original (it arrived before this device joined, or
was pruned), and a quote must stand alone. Duration is one `uint32`; it is cheap
and makes the bubble preview complete without a lookup. When the original *is*
present locally, the app may still prefer the live value; the wire value is the
floor.

### D3: Composer keeps the mic while a reply is pending (gap A, UI)

`_micShown` currently is `textEmpty && pickImage-ish && widget.reply == null`.
Drop the `widget.reply == null` term so the mic shows whenever the text field is
empty, reply pending or not. When the user records while a reply is pending, the
send path calls `send_audio_in(chat, wav, reply_to: reply.id)` and the reply bar
clears, exactly like a text reply. Typing text while a reply is pending still swaps
the mic for Send (text reply), unchanged.

## Wire / core change (minimal, summarised)

| Layer | Change |
|---|---|
| `proto/vyajena/v1/content.proto` | `Audio.reply_to = 6`; `ReplyTo.original_kind = 4`, `ReplyTo.original_duration_ms = 5`; new `enum OriginalKind`. `pbgen` regenerates the committed `pb.rs`. |
| `client` core | `reply_ref` fills `original_kind` + `original_duration_ms`; new `encode_audio`-with-reply path; `send_audio_in` gains `reply_to: Option<MessageId>`; decode maps `Audio.reply_to` into `AudioContent`/event `Reply`. |
| `bridge` | `send_audio_in(chat, wav, reply_to)` gains the id; the event `Reply` gains `kind` + `duration_ms`. `flutter_rust_bridge_codegen` regenerates bindings. |
| `app` | `_micShown` drops the `reply == null` term; audio send passes the pending reply id; the quote block renders from `original_kind` (mic glyph + duration for audio), in both the reply bar and the bubble. |
| server / relay / wire envelope | **none** — all inside the encrypted `Content`. |

### Field-number coordination

The additions are to `ReplyTo` (fields 4, 5) and `Audio` (field 6) and a new enum;
they do **not** touch the `Content.body` oneof, whose next free number (7) is taken
by `delete-messages`. So `voice-replies` does not contend for `Content.body` field
numbers with `delete-messages`, `video-messages`, `audio-calls`, `replace-user`,
`mute-notifications`, or `notification-reliability`. `buf breaking --against main`
is run to confirm the additions are backward-compatible.

## Risks / Trade-offs

- **[Old app receives a voice reply]** → it ignores `Audio.reply_to` and plays the
  voice message with no quote shown. Acceptable: no error, no placeholder, the
  voice still plays.
- **[Old app receives a new-style text/image/audio quote]** → `original_kind` is an
  unknown field to it; it falls back to the old `quote.isEmpty ? unsupported`
  behaviour, i.e. unchanged from today (the bug persists only against old
  receivers, which is unavoidable without a wire field they understand).
- **[Localised label in content]** → avoided: the wire carries `original_kind`
  (an enum) and duration, not a localised "Voice message" string. The receiver's
  own app localises the label.

## Migration Plan

Proto-first: edit `content.proto`, run `cargo run -p pbgen`, then the core,
bridge (`flutter_rust_bridge_codegen generate`), and app, in that order. No data
migration: `ReplyTo` rows already stored keep working (new fields default to
`UNSPECIFIED`/0). No server deploy. Builds serialise on the shared external-disk
cargo target.

## Open Questions

- None blocking. A future change may add `reply_to` to `Image` the same way (D1),
  and a localised duration format is a UI detail, not a spec decision.
