## 1. Server — durable catch-up counter (D5)

- [ ] 1.1 Add `CATCH_UP_MAX` to `protocol/` (message_delivery), beside
      `WAKE_PING_MIN_INTERVAL`
- [ ] 1.2 Add migration `0009_catchup.sql` (table `catchup(device_id BLOB PRIMARY
      KEY, attempts INTEGER NOT NULL)`) and register it in `migrations.rs`
- [ ] 1.3 `log.rs` helpers: `has_backlog`, `backlogged_devices`,
      `catchup_attempts`, `bump_catchup`, and `prune_catchup(connected)` (reset the
      counter for drained/expired and currently-connected devices)

## 2. Server — re-ping on stream loss (D1)

- [ ] 2.1 On `Connect` stream close / idle-timeout reap, if the device still has
      undelivered queued entries and a push token, request a wake push (shared
      one-per-device-per-60 s limit, via the existing `notify` path); test
      `message-delivery/re-ping-on-stream-loss`

## 3. Server — sweep catch-up (D2 + D5)

- [ ] 3.1 In the sweep, prune the counter (D5 reset), then for each offline device
      with an undelivered backlog, a token, and attempts `< CATCH_UP_MAX`, add it to
      the wake-push set and bump its counter; repeats per sweep until the backlog
      drains or the bound is hit; tests `message-delivery/sweep-catch-up-ping`,
      `message-delivery/catch-up-stops-when-drained`,
      `message-delivery/catch-up-bound-survives-restart`

## 4. Client — prompt reconnection (D4, already implemented)

- [ ] 4.1 No new code: the engine already reconnects after a dropped stream with a
      bounded backoff and exposes `reconnect_now`, and the app lifecycle restarts
      the core on resume and reconnects on network regain. Add/confirm a core test
      pinning reconnect-after-a-dropped-stream; test
      `client-runtime/reconnect-after-a-dropped-stream`

## 5. Check

- [ ] 5.1 `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo fmt --check`, `flutter analyze`, `flutter test`,
      `openspec validate notification-reliability --strict` (builds serialized on
      the shared cargo target)
