## 1. Proto and shared limits

- [x] 1.1 `content.proto`: `Text.mentions`, `Text.reply_to`, `Mention` (user | everyone, `start`, `length`), `ReplyTo` (`sender`, `msg_id`, `quote`); `buf lint`, `buf breaking`; `cargo run -p pbgen`
- [x] 1.2 `protocol::message_content`: `MAX_REPLY_QUOTE_CHARS`, `reply_quote`; test `message-content/replies` (folding, cut, "…")

## 2. Core

- [x] 2.1 `messaging`: encode/decode mentions and `reply_to`, span checks (send refuses, receive drops), quote cut on receive; tests `message-content/mentions`, `message-content/text-only`, old-client decode (D8)
- [x] 2.2 Store migration `0005_mentions.sql` (`mention_unseen`); `Client::send_message` (mentions + reply to a local id), `unseen_mentions`, `mark_mention_seen`; `Message.mentions`, `mentions_me`, `reply`
- [x] 2.3 End-to-end tests: `client-runtime/reply-lookup`, `client-runtime/unseen-mentions`, `message-content/mentions` (everyone from a non-admin), `message-content/replies` (device without the original), `message-content/mentions-and-replies-stay-encrypted` (push text)

## 3. Bridge

- [x] 3.1 `MentionSpan` (UTF-16 offsets), `MentionTarget`, `ReplyQuote`, `ChatMessage.mentions/mentions_me/reply`, `send_message`, `unseen_mentions`, `mark_mention_seen`; UTF-8↔UTF-16 tests; `flutter_rust_bridge_codegen generate`

## 4. App

- [x] 4.1 Composer: mention picker and mention tracking; reply bar; widget tests `app-ui/mention-picker`
- [x] 4.2 Message tile: mention spans, highlight of mine, reply quote; chat `@` badge and seen marking; widget tests `app-ui/mentions-of-me`
- [x] 4.3 Swipe and long-press to reply, jump to the original; widget tests `app-ui/reply-to-a-message`

## 5. Check

- [ ] 5.1 `cargo test --workspace`, clippy, fmt, `buf lint`, `flutter analyze`, `flutter test`, `openspec validate mentions --strict`
