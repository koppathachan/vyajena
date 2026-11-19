## 1. Redaction helper

- [x] 1.1 `server/src/runtime/redact.rs`: `redact(&[u8]) -> impl Display` =
      first 12 hex of `SHA-256(bytes)`; unit test for stability and that it is
      not a prefix of the input (spec: pi-runtime/private-logs).
- [x] 1.2 Wire it into `server/src/runtime/mod.rs` (export).

## 2. Redact the existing id logs

- [x] 2.1 `server/src/messaging/stream.rs` (open/close): `device = %redact(...)`
      instead of `hex(...)`.
- [x] 2.2 `server/src/devices/mod.rs` (link/revoke): same.
- [x] 2.3 `server/src/push/mod.rs` (wake ping): redact `%device_id`.
- [x] 2.4 Quick audit of `client/` and `deploy/`: redact any raw id a log prints
      (none logs a body/token/invite — keep it that way).

## 3. Connection metrics

- [ ] 3.1 Time the lifecycle in the stream/auth path: `auth_ms`, `stream_open_ms`
      (and `connect_ms` where the TCP-accept time is reachable); a per-device
      reopen counter; read the hub's live-stream count for `active_streams`.
- [ ] 3.2 Emit one structured `tracing` event on stream open with those fields,
      ids redacted (spec: pi-runtime/connection-metrics). Log `active_streams`
      when it changes.

## 4. Tests

- [ ] 4.1 A `tracing` capture layer (collect emitted events into a `Vec<String>`)
      in `server/tests/support/`.
- [ ] 4.2 `server/tests/` test (temp Pi): run connect → auth → stream; assert the
      capture contains the `redact(id)` hash and `auth_ms`/`stream_open_ms`/
      `active_streams`, and contains NO raw device-id hex, body, token or invite
      (spec: pi-runtime/private-logs, pi-runtime/connection-metrics).

## 5. Verify

- [ ] 5.1 `cargo build/test/clippy --workspace -- -D warnings`, `cargo fmt
      --check`.
- [ ] 5.2 `buf lint`, `buf breaking --against '.git#branch=main'` (clean — no
      proto change).
- [ ] 5.3 `openspec validate relay-privacy-logging --strict`.
- [ ] 5.4 Report the commit list to the owner; do not push without approval.
