## 1. Redaction helper

- [x] 1.1 `server/src/runtime/redact.rs`: `redact(&[u8]) -> impl Display` =
      first 12 hex of `SHA-256(bytes)`; unit test for stability and that it is
      not a prefix of the input (spec: server-runtime/private-logs).
- [x] 1.2 Wire it into `server/src/runtime/mod.rs` (export).

## 2. Redact the existing id logs

- [x] 2.1 `server/src/messaging/stream.rs` (open/close): `device = %redact(...)`
      instead of `hex(...)`.
- [x] 2.2 `server/src/devices/mod.rs` (link/revoke): same.
- [x] 2.3 `server/src/push/mod.rs` (wake ping): redact `%device_id`.
- [x] 2.4 Quick audit of `client/` and `deploy/`: redact any raw id a log prints
      (none logs a body/token/invite — keep it that way).

## 3. Connection metrics

- [x] 3.1 Hub: a per-device reopen counter (returned from `register`) and an
      `active()` live-stream count; measure `open_ms` in the Connect task from
      its start to `register`. (auth_ms/connect_ms dropped — see design D2.)
- [x] 3.2 Emit one structured `tracing` event on stream open with `open_ms`,
      `reopens`, `active_streams`, id redacted (spec: server-runtime/connection-metrics).

## 4. Tests

- [x] 4.1 A `tracing` capture writer (collect emitted events into a shared
      buffer) in the test, following the existing `invite_logs.rs` pattern.
- [x] 4.2 `server/tests/private_logs.rs` (temp server): run connect → auth → stream;
      assert the capture contains the `redact(id)` hash and `open_ms`/`reopens`/
      `active_streams`, and contains NO raw device-id hex, nor the session token
      (spec: server-runtime/private-logs, server-runtime/connection-metrics).
      `push_end_to_end.rs` updated to assert the redacted id, not the raw hex.

## 5. Verify

- [x] 5.1 `cargo build/test/clippy --workspace -- -D warnings`, `cargo fmt
      --check`.
- [x] 5.2 `buf lint`, `buf breaking --against '.git#branch=main'` (clean — no
      proto change).
- [x] 5.3 `openspec validate relay-privacy-logging --strict`.
- [ ] 5.4 Report the commit list to the owner; do not push without approval.
