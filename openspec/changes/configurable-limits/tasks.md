# Tasks — configurable-limits

## 1. Config schema (server)
- [ ] 1.1 Add a `Limits` struct to `server/src/runtime/config.rs` with one field per
      exposed limit; each `Option` in the raw TOML, resolved to the current const
      when absent. Add the fields to the unknown-field allow-list (`field::ALL`).
- [ ] 1.2 Parse `[limits]` (durations as seconds / `*_days`; counts and bytes as
      integers), validate each (> 0, sane bounds), refuse to start naming a bad
      field, as other settings do.
- [ ] 1.3 Default every field to its current value: `INACTIVITY_TIMEOUT`,
      `QUEUE_ENTRY_TTL`, `QUEUE_CAP`, `STREAM_IDLE_TIMEOUT`, `INVITE_TTL`,
      `BLOB_TTL`, `CHALLENGE_TTL`, `DEDUP_MIN_RETENTION`, `WAKE_PING_MIN_INTERVAL`,
      `CATCH_UP_MAX`, `SWEEP_INTERVAL`, `MAX_BLOB_STORE_BYTES`,
      `MAX_UPLOADS_PER_DEVICE_PER_HOUR`.

## 2. Thread values to the call-sites (server)
- [ ] 2.1 Replace each server-side const read with the resolved runtime value:
      sweep (`inactivity_timeout`, `queue_entry_ttl`, `sweep_interval`), write path
      (`queue_cap`), stream (`stream_idle_timeout`), invites (`invite_ttl`), blobs
      (`blob_ttl`, `max_blob_store_bytes`, `max_uploads_per_device_per_hour`,
      blob sweep), devices/auth (`challenge_ttl`), dedup retention, the wake-ping
      limiter (`wake_ping_min_interval`) and catch-up bound (`catch_up_max`).
      Leave the `protocol` consts in place as the defaults; the client keeps using
      them.

## 3. Startup visibility
- [ ] 3.1 Log the effective configuration once at startup (every limit + the
      non-secret base settings). Never log keys, secret paths or content.
- [ ] 3.2 Document every limit (default + unit + trade-off) in the example TOML and
      the `config.rs` doc comment.

## 4. Tests
- [ ] 4.1 Absent `[limits]` reproduces current behaviour (defaults equal the consts).
      // spec: server-runtime/configuration
- [ ] 4.2 A set value takes effect (e.g. a short `queue_entry_ttl` drops an entry
      sooner; a small `queue_cap` marks inactive sooner).
      // spec: server-runtime/configuration
- [ ] 4.3 A bad value (zero/negative/out of range/unknown field) refuses start,
      naming the field. // spec: server-runtime/configuration
- [ ] 4.4 The startup log lists the effective values.
      // spec: server-runtime/effective-configuration-is-logged-at-startup

## 5. Done
- [ ] 5.1 `cargo test`, `cargo clippy -D warnings`, `cargo fmt --check`,
      `openspec validate configurable-limits --strict` pass; defaults unchanged so
      safe to deploy; a Pi restart applies new values.
