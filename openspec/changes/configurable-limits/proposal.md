## Why

The constants that decide when the server removes an idle device, drops a queued
message, expires an invite or a blob, and rate-limits pushes are **compile-time
consts** in the `protocol` crate (`INACTIVITY_TIMEOUT`, `QUEUE_ENTRY_TTL`, …, all
7 days / fixed). The deployed server wedged a real user after 7 days of iOS
background (group-resync), and the operator (Akhil) had **no way to see or tune
these values** — he did not even know the 7-day limit was in force. Changing one
today means editing a const and redeploying; there is no visibility into the
effective values at all.

This change makes the **server-enforced** limits operator-configurable in the
server TOML, **each defaulting to its current value**, and makes the server **log
the effective values at startup** so they can be seen. Defaults are unchanged, so
the change is behaviour-neutral out of the box and safe to deploy at any time; an
operator can then raise, e.g., `inactivity_timeout` to 30 days (noting the
trade-offs in design.md) without a code change.

## What Changes

- **A `[limits]` TOML table (server).** The server SHALL read an optional
  `[limits]` table; every field is optional and **defaults to today's constant**,
  so an absent table or field keeps current behaviour exactly. The server SHALL
  validate each (positive, within sane bounds) and refuse to start naming a bad
  field, as it already does for other settings.
- **Effective configuration logged at startup (server).** The server SHALL log,
  once at startup, the effective value of every configurable limit (and the other
  non-secret settings), so the operator can see what is in force without reading
  the source. Nothing secret is logged.
- **Example TOML + doc comments.** The example config and `config.rs` doc comment
  SHALL document every limit with its default and unit.

### Limits exposed (server-enforced only; each defaults to the current const)

- `inactivity_timeout` — `INACTIVITY_TIMEOUT` (7 days): idle before a device is
  removed.
- `queue_entry_ttl` — `QUEUE_ENTRY_TTL` (7 days): how long an undelivered queued
  entry is kept before it is dropped (and a gap notice sent).
- `queue_cap` — `QUEUE_CAP` (1000): per-device queue length before the device is
  marked inactive.
- `stream_idle_timeout` — `STREAM_IDLE_TIMEOUT`: idle before a `Connect` stream is
  reaped.
- `invite_ttl` — `INVITE_TTL` (7 days).
- `blob_ttl` — `BLOB_TTL` (7 days).
- `challenge_ttl` — `CHALLENGE_TTL`: auth challenge validity.
- `dedup_min_retention` — `DEDUP_MIN_RETENTION` (7 days).
- `wake_ping_min_interval` — `WAKE_PING_MIN_INTERVAL` (60 s).
- `catch_up_max` — `CATCH_UP_MAX` (5): bounded catch-up push attempts.
- `sweep_interval` — `SWEEP_INTERVAL` (60 s).
- `max_blob_store_bytes` — `MAX_BLOB_STORE_BYTES` (1 GiB).
- `max_uploads_per_device_per_hour` — `MAX_UPLOADS_PER_DEVICE_PER_HOUR` (60).

Client-side timings (own-leaf rotation, re-add request window, key-package
targets) stay compile-time: they are enforced on the device, not the server, and
changing them would need a client release, not a server config.

## Open decisions resolved

- Owner (Akhil, 2026-11-25): expose the server limits as config, applied on
  restart, **defaults = current values** (no behaviour change out of the box);
  log them so they are visible. Raising `inactivity_timeout` is noted as the
  operator's lever against the wedge, with its trade-offs in design.md.

## Proto files touched

- **None.** This is server configuration only; no wire field, no client change.
  It contends for no proto field numbers with any in-flight change.

## Capabilities

### Modified Capabilities

- `server-runtime`: Configuration gains the optional `[limits]` table (defaults =
  current), and the server logs the effective configuration at startup.

## Impact

- **Server:** `server/src/runtime/config.rs` (parse + validate `[limits]`,
  defaults = current consts; startup log of the effective config), and the
  call-sites that read each const now read the runtime value (e.g.
  `server/src/messaging/sweep.rs`, `messaging/write.rs` queue cap,
  `messaging/stream.rs` idle timeout, `admin/invites.rs` invite TTL,
  `blobs/mod.rs` blob TTL + store/upload limits + sweep, `devices/mod.rs`
  challenge TTL, the wake-ping limiter and catch-up bound). Where a const lives in
  `protocol`, the server threads a runtime value defaulting to that const rather
  than editing the const.
- **Tests:** `server/tests/` — an absent `[limits]` keeps current behaviour; a set
  value takes effect (e.g. a shorter `queue_entry_ttl` drops sooner); a bad value
  is refused naming the field; the startup log lists the effective values.
- **Docs/ops:** example TOML documents every limit; **applied on restart**; a Pi
  redeploy/restart is needed for a change to take effect. No migration, no new
  credentials, no relay change. Pairs with `group-resync`.
