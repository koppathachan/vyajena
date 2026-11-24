## Why

The 2026-11-24 self-hosting test found two coupled gaps around replies and voice
messages:

- **You cannot reply *with* a voice note.** Swiping to reply hides the mic
  (`app/lib/chat/composer.dart` `_micShown` requires `widget.reply == null`), and
  `reply_to` lives only on the `Text` body, so a voice message can never answer
  another message.
- **Quoting a voice message shows "needs a newer version".** The core stores an
  empty quote for a voice original (`client/src/client.rs::reply_ref`), and the app
  renders an empty quote as the "needs a newer app version" placeholder
  (`app/lib/chat/message_tile.dart`), even though the composer bar correctly says
  "Voice message".

Both are client-side content inside the encrypted `Content`: the server stores no
history and never sees a quote, so the fix stays within the E2EE model and touches
no server, relay or wire-envelope code.

## What Changes

- **A voice message can carry a reply-to (message-content: Voice replies).** The
  `Audio` body gains an optional `reply_to` of the same `ReplyTo` shape a text
  reply uses. The core builds it from the answered message exactly as for text. An
  old app ignores the field and still plays the voice message.
- **The reply quote describes the original's kind (message-content: Reply quote
  describes the original's kind).** `ReplyTo` gains `original_kind` (text / image /
  audio) and `original_duration_ms`, so a receiver that does not hold the original
  still renders a correct preview. The empty-quote-means-unsupported rule is
  removed; the placeholder is reserved for a genuinely unknown body.
- **The composer keeps the mic while a reply is pending (app-ui: Reply with a voice
  message).** `_micShown` drops the `reply == null` term; recording with a reply
  pending sends a voice message answering it and clears the reply bar. Typing text
  with a reply pending still makes a text reply, unchanged.
- **Voice-message reply preview (app-ui: Voice-message reply preview).** The reply
  bar and the bubble render a voice original as a mic glyph plus duration, a
  captionless picture as a picture preview, and text as the quoted text, driven by
  `original_kind`.
- Text replies are unchanged on the wire and in the UI.

## Proto files touched

- `proto/vyajena/v1/content.proto`:
  - `Audio.reply_to = 6` (field 6, next free on `Audio`).
  - `ReplyTo.original_kind = 4`, `ReplyTo.original_duration_ms = 5`.
  - new `enum OriginalKind` (UNSPECIFIED / TEXT / IMAGE / AUDIO).
- These additions are to `ReplyTo` and `Audio`, not to the `Content.body` oneof, so
  they do **not** contend for `Content.body` field numbers with `delete-messages`
  (which takes `body = 7`), `video-messages`, `audio-calls`, `replace-user`,
  `mute-notifications`, or `notification-reliability`.
- `cargo run -p pbgen` regenerates the committed `pb.rs`; `buf breaking --against
  main` confirms the additions are backward-compatible.

## Capabilities

### New Capabilities

<!-- None. -->

### Modified Capabilities

- `message-content`: new requirements "Voice replies" and "Reply quote describes
  the original's kind".
- `app-ui`: new requirements "Reply with a voice message" and "Voice-message reply
  preview".

## Impact

- **Proto / generated code:** `content.proto` + regenerated `pb.rs` (`pbgen`) and
  Flutter bindings (`flutter_rust_bridge_codegen generate`).
- **Core (`client/`):** `reply_ref` fills the new `ReplyTo` fields; an audio send
  path that carries a reply (`send_audio_in` gains `reply_to`); decode maps
  `Audio.reply_to` into the audio event.
- **Bridge (`bridge/`):** `send_audio_in` gains `reply_to`; the reply event gains
  `kind` + `duration_ms`.
- **App (`app/lib/chat/`):** `composer.dart` `_micShown`; the audio send passes the
  pending reply id; `message_tile.dart` / reply-bar render from `original_kind`.
- **Server / relay / wire envelope:** none.
