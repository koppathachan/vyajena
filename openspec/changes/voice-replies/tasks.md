# Tasks

Proto-first, then core, bridge, app. Builds serialise on the shared external-disk
cargo target. Tick a task only when its tests pass.

## 1. Proto (message-content: Voice replies; Reply quote describes the original's kind)

- [ ] 1.1 `proto/vyajena/v1/content.proto`: add `ReplyTo.original_kind = 4`
  (`enum OriginalKind { UNSPECIFIED, TEXT, IMAGE, AUDIO }`),
  `ReplyTo.original_duration_ms = 5`, `ReplyTo.original_thumbhash = 6`; add
  `Audio.reply_to = 6`.
- [ ] 1.2 `cargo run -p pbgen` and commit the regenerated `pb.rs` files.
- [ ] 1.3 `buf lint` and `buf breaking --against '.git#branch=main'` are clean.

## 2. Core (message-content: Voice replies; Reply quote describes the original's kind)

- [ ] 2.1 `reply_ref` (`client/src/client.rs`): set `original_kind` from the
  decoded original body, `original_duration_ms` from an audio original's duration,
  and `original_thumbhash` from a picture original's ThumbHash (0/empty otherwise);
  keep the text quote (and a picture's caption) as today.
- [ ] 2.2 Encode/decode (`client/src/messaging/mod.rs`): carry the new `ReplyTo`
  fields; add an `encode_audio`-with-reply path; map `Audio.reply_to` on decode
  into the audio content / event.
- [ ] 2.3 `send_audio_in` (`client/src/client.rs`): accept `reply_to:
  Option<MessageId>`, build the `ReplyTo` via `reply_ref`, and put it on the `Audio`
  body. Keep text and image replies unchanged.
- [ ] 2.4 Event `Reply` (`client/src/events.rs`): carry the original's kind and
  duration so the UI can render the preview without the original.
- [ ] 2.5 Core tests (`client/tests/`): `// spec: message-content/voice-replies`
  and `// spec: message-content/reply-quote-describes-the-original-s-kind` — a voice
  reply round-trips its `reply_to`; a reply to a voice original carries kind=AUDIO
  and the duration with an empty text quote; text replies unchanged.

## 3. Bridge

- [ ] 3.1 `bridge/src/api/core.rs`: `send_audio_in` (and `send_audio`) gain
  `reply_to: Option<i64>`, forwarded to the core.
- [ ] 3.2 `bridge/src/api/types.rs` / events: the reply view carries `kind` and
  `duration_ms`.
- [ ] 3.3 `cd app && flutter_rust_bridge_codegen generate` and commit the
  regenerated bindings.
- [ ] 3.4 Bridge tests pass.

## 4. App (app-ui: Reply with a voice message; Media reply preview)

- [ ] 4.1 `app/lib/chat/composer.dart`: `_micShown` drops the `widget.reply == null`
  term (mic shows whenever the text field is empty).
- [ ] 4.2 Audio send passes the pending reply id (`chat_screen.dart`); the reply bar
  clears after sending, like a text reply.
- [ ] 4.3 `app/lib/chat/message_tile.dart` + reply bar: render the quote from
  `original_kind` — audio → mic glyph + duration + "Voice message"; image →
  ThumbHash thumbnail + caption; text → quoted text. Remove the `quote.isEmpty →
  unsupportedText` rule so a known media kind never shows the placeholder.
- [ ] 4.4 Widget tests (`app/test/`): a voice reply preview renders a mic glyph +
  duration (reply bar and bubble, original present and absent); the mic stays while
  a reply is pending; text replies unchanged.

## 5. Validate / verify

- [ ] 5.1 `cargo test --workspace`, `cargo clippy --workspace --all-targets -D
  warnings`, `cargo fmt --check`, `cd app && flutter analyze && flutter test`.
- [ ] 5.2 `openspec validate voice-replies --strict` passes.
- [ ] 5.3 Simulator: swipe-reply a voice message → mic visible → record+send →
  recipient shows a voice reply quoting the voice original with a mic glyph +
  duration (coordinate with the live test session; serialise builds).
