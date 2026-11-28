# Tasks — wake-push-timeliness

Server-only. No proto, no client, no app release. Do in order; tick a task only when
its tests pass. Run `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo fmt --check` and `openspec validate wake-push-timeliness --strict` before done.

## 1. Constants (protocol)
- [x] 1.1 Add `HTTP2_KEEPALIVE_INTERVAL` (20 s) and `HTTP2_KEEPALIVE_TIMEOUT` (10 s)
      to `protocol/src/wire_transport.rs`.
- [x] 1.2 Add `NEW_MESSAGE_PING_MIN_INTERVAL` (10 s) to
      `protocol/src/message_delivery.rs`, next to `WAKE_PING_MIN_INTERVAL`.

## 2. Fix 1 — HTTP/2 keepalive on the Connect transport
- [x] 2.1 `server/src/runtime/config.rs`: add operator-tunable
      `http2_keepalive_interval` and `http2_keepalive_timeout` limits (default to the
      protocol constants), parsed like `stream_idle_timeout`, and add them to the
      startup effective-configuration log line.
- [x] 2.2 `server/src/runtime/mod.rs` (`Server::builder()` ~:247): set
      `.http2_keepalive_interval(Some(..))` and `.http2_keepalive_timeout(Some(..))`
      from the config.
- [x] 2.3 Test: the keepalive is a tonic transport option with no in-process getter,
      and a true half-open socket can't be simulated against the in-memory harness,
      so there is no new unit test for it. The behaviour it accelerates — a lost
      stream with a backlog gets a re-ping — is already covered by
      `server/tests/notification_reliability.rs`
      (`a_backlog_at_stream_loss_gets_a_catch_up_ping`). Transport-level detection is
      verified by the staging check in 2.4.
- [ ] 2.4 Manual/staging check (owner or test session): a killed client's connection
      is closed within ~30 s **through the rathole relay**; note the result in
      `design.md`.

## 3. Fix 2 — reason-aware rate limiter
- [x] 3.1 `server/src/push/mod.rs`: add `enum PingClass { Message, Nudge }`; give
      `PingLimiter` per-device `last_message_at` + `last_any_at`; change
      `take(device_id, now, class, msg_interval, nudge_interval) -> bool` with the
      rule in `design.md` (Message floored by `last_message_at`+10 s and ignores
      nudges; Nudge floored by `last_any_at`+60 s and does not reset the message
      floor).
- [x] 3.2 `server/src/messaging/mod.rs`: carry `PingClass` into `ping_offline`;
      `notify`/`notify_in` default to `Message`; `notify_own_access` passes `Nudge`;
      pass the class to `PingLimiter::take`.
- [x] 3.3 `server/src/messaging/sweep.rs`: the catch-up ping path passes `Nudge`.
- [x] 3.4 `server/src/messaging/stream.rs`: the stream-loss re-ping (~:569) passes
      `Message`.
- [x] 3.5 Tests (`server/tests/`, `// spec: message-delivery/content-free-wake-pings`):
      a recent nudge does not starve a new message (10 s floor); two messages <10 s →
      one ping; two nudges <60 s → one ping.

## 4. Observability (folded in)
- [x] 4.1 `server/src/messaging/mod.rs`: emit a `debug` "wake ping skipped" line with a
      `reason` (`connected` / `no_notifiable_backlog` / `muted` / `rate_limited`(+class))
      at each skip, keeping the existing `sent`/`dropped`/`token_invalid` outcome
      logs. Debug level (enable with `RUST_LOG=server=debug` to diagnose a miss); no
      behaviour change. (A promoted-level counter/metric is a follow-up, not in scope.)

## 5. Gate
- [x] 5.1 `cargo test -p server` green (53 binaries, 383 tests, 0 failures).
- [x] 5.2 `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`,
      `buf lint` clean.
- [x] 5.3 `openspec validate wake-push-timeliness --strict` passes.
- [x] 5.4 No proto/client/app files changed (server + protocol + openspec only).
