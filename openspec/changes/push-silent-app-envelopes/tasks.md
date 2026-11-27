## 1. Proto

- [x] 1.1 Add `bool silent = 12;` to `Envelope` (`proto/vyajena/v1/messaging.proto`)
  in the client-set section, documenting that the sender sets it for an
  `APPLICATION` envelope whose `Content` has no user-visible message
  (`Profile`/`GroupSettings`/`Delete`), and that an old server ignores it.
- [x] 1.2 Regenerate with `cargo run -p pbgen`; commit the `pb.rs` changes.
- [x] 1.3 `buf lint` clean; `buf breaking --against '.git#branch=main'` clean
  (additive field).

## 2. Client

- [x] 2.1 Add `messaging::is_silent_content(content) -> bool`, true for
  `Decoded::Profile | Decoded::GroupSettings { .. } | Decoded::Delete`.
- [x] 2.2 In `engine::outbox`, set `silent: is_silent_content(&message.content)` on
  the `APPLICATION` envelope it builds (beside `deletes`).

## 3. Server

- [x] 3.1 Migration `0013_silent_envelopes.sql`: `ALTER TABLE envelopes ADD COLUMN
  silent INTEGER NOT NULL DEFAULT 0 CHECK (silent IN (0, 1))`.
- [x] 3.2 `log::NewEnvelope` gains `silent: bool`; `log::insert_envelope` writes the
  column; the client-submit path passes `envelope.silent`, the server external-Remove
  path passes `false`.
- [x] 3.3 Narrow `log::has_application_backlog` and `log::backlogged_devices` with
  `AND e.silent = 0`; update their doc comments to say "notifiable". Leave
  `application_queue_len` and `expire` unfiltered.

## 4. Test

- [x] 4.1 `server/tests/wake_pings.rs`: a silent `APPLICATION` envelope (a profile
  update) for an offline device fires no wake ping, while a following real message
  fires exactly one — tagged `// spec: message-delivery/content-free-wake-pings`.
- [x] 4.2 A silent-only backlog spends no catch-up budget (sweep sends no ping),
  reusing the notification-reliability harness.

## 5. Verify

- [x] 5.1 `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --check`, `buf lint`.
- [x] 5.2 `openspec validate push-silent-app-envelopes --strict`.
