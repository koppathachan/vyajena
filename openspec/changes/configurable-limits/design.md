# Design — configurable-limits

## Approach

Add an optional `[limits]` table to the server TOML. Each field is optional; a
missing field uses the current `protocol` (or server) constant as its default, so
an absent table reproduces today's behaviour byte-for-byte. Parsing reuses the
existing `Config` machinery (`server/src/runtime/config.rs`): unknown-field
rejection, per-field validation, and start-time refusal naming the bad field.

Durations are written in the TOML as whole seconds (or a `*_days` convenience
where natural), parsed into `Duration`. Counts and byte sizes are integers.
Validation: each must be > 0 and within a sane upper bound (e.g.
`wake_ping_min_interval` ≥ 1 s; `queue_cap` ≥ 1; `inactivity_timeout` ≥ 1 hour).

Threading: the server holds the resolved values on its runtime context and reads
them where it reads the const today. For a const that lives in the shared
`protocol` crate (used by both sides), the **server** reads the runtime value
while the **client** keeps the const — only server enforcement is configurable.
No const is deleted; it remains the documented default.

## Startup visibility ("I just need to SEE it")

At startup the server logs one structured line per effective limit (plus the
non-secret base settings: server id, admin username, listen/public address, blob
backend). Never logs keys, paths of secrets, or content. This is the operator's
window into what is in force, independent of tuning.

## Trade-offs the operator should know (documented in the example TOML)

- **`inactivity_timeout` (raise to e.g. 30 days to avoid the wedge):** inactive
  leaves then linger longer in the MLS ratchet tree (larger tree, more server
  Remove proposals outstanding), and the server keeps per-device queue state
  longer. Bounded by `queue_cap` (a busy group still evicts a quiet device once
  its queue fills).
- **Relationship to `queue_entry_ttl`:** a returning device only receives messages
  still within `queue_entry_ttl`. Raising `inactivity_timeout` without raising
  `queue_entry_ttl` means a device back after > `queue_entry_ttl` is kept in the
  group but still missed the older messages (a gap notice covers them). To truly
  keep messages for a longer absence, raise both — at the cost of server storage
  (undelivered ciphertext retained longer) and a larger dedup table
  (`dedup_min_retention` should be ≥ `queue_entry_ttl`).
- **`stream_idle_timeout` / `wake_ping_min_interval` / `catch_up_max`:** tuning
  push aggressiveness vs. battery and APNs/FCM rate limits (see
  notification-reliability).
- **`max_blob_store_bytes`:** disk ceiling for the blob store; the blob sweep
  (`blob_ttl`, `sweep_interval`) reclaims expired blobs.

## Non-goals

- Client-side timings (own-leaf rotation age/interval/jitter, re-add request
  window and daily cap, key-package targets) are **not** server-configurable:
  they are enforced on the device and would need a client release and possibly a
  wire signal to vary per server. Out of scope.
- No live reload: limits are applied **on restart** (owner decision). A hot
  reload is a possible later refinement, not needed now.
- The MLS invariants are untouched (strict epoch, `max_past_epochs = 0`); these
  are operational limits, not protocol semantics.
