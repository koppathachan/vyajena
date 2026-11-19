## Why

The Pi logs raw `device_id`s in full hex (stream open/close, device link/revoke,
wake pings). A `device_id` is a stable per-device identifier; printing it in the
clear lets anyone with the journal correlate a device across every log line and
over time, which undercuts the project's privacy stance (the Pi is meant to see
as little as possible). Separately, there is no way to diagnose the "slow to
connect over the relay" problem from the Pi: no timings for the connect / auth /
stream-open steps, no reconnect counts, no view of how many streams are live.

## What Changes

- **Redact ids in logs.** Add a small reusable redaction helper and replace
  every raw/full-hex `device_id` in a log with a **short stable hash prefix**
  (correlatable across lines, not the raw id). Sites today:
  `messaging/stream.rs` (open/close), `devices/mod.rs` (link/revoke),
  `push/mod.rs` (wake ping).
- **Never log secrets or content.** Make it a rule (and a test) that the Pi
  logs and metrics never contain a message body/ciphertext, a session token, or
  an invite code. (An audit found none today; this keeps it that way.)
- **Add privacy-preserving connection metrics.** Emit structured
  connection-lifecycle events so slow-connect issues are diagnosable from the
  journal: per-connection **connect / auth / stream-open timings**, a
  **reconnect count**, and an **active-stream gauge** (the Pi-side view of
  "pool" health). All ids in them are redacted; no new network endpoint.
- Logging/metrics only — **no `.proto` or wire change** (`buf breaking` clean).

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
- `pi-runtime`: the Pi gains two requirements — private logs (ids redacted,
  never bodies/tokens/invites) and connection metrics (redacted lifecycle
  timings and counts emitted as structured events).

## Impact

- `server/src/runtime/` — a new `redact.rs` helper (short stable hash of an id);
  `server_keys::hex` stays for non-id byte formatting.
- `server/src/messaging/stream.rs`, `server/src/devices/mod.rs`,
  `server/src/push/mod.rs` — swap raw/hex ids for the redacted form; add the
  lifecycle timing/count events in the stream and auth paths.
- `server/tests/` — a test that drives a connect/auth/stream and asserts the
  captured logs/metrics contain no raw `device_id`, body, token or invite, and
  do carry the redacted id and the timing fields.
- `client/`, `deploy/` — a quick audit; redact any id a log there prints.
- **Scope boundary** (design D3): the relay↔Pi backhaul is rathole (a separate
  binary), so "backhaul pool" metrics on the Pi are the active-stream count, not
  rathole's internal pool.
