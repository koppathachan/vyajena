## 1. Make the wake push kind-aware (server)

- [x] 1.1 Add `log::has_application_backlog(device_id)` and narrow
  `log::backlogged_devices` to devices with a pending `APPLICATION` delivery.
- [x] 1.2 In `notify_in`'s offline branch, skip the wake push for a device with no
  `APPLICATION` backlog; leave the open-stream wake and the queued entries
  untouched.
- [x] 1.3 Update the `notify` / `log` doc comments to state the kind-aware rule.

## 2. Test

- [x] 2.1 `server/tests/wake_pings.rs`: a control-only backlog (an offline
  device's queued `COMMIT`) produces no wake push, while a later `APPLICATION`
  message does — tagged `// spec: message-delivery/content-free-wake-pings`.

## 3. Verify

- [x] 3.1 `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt
  --check`, the new and existing wake-ping / notification-reliability tests.
- [x] 3.2 `openspec validate push-kind-aware --strict`.
