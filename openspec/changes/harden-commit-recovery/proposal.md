## Why

When this device stages its own membership commit, it writes the staged commit to
the `outgoing_commit` row so a restart can finish it (the group keeps exactly one
pending commit; `pending_commit().is_some()`). The row carries a `purpose` string
that says which job produced the commit, so the restart re-runs the same job.

That `purpose` is a hand-rolled, pipe-delimited string codec
(`Job::purpose()` / `Job::recovered()` in `client/src/engine/jobs.rs`), and it has
two defects (audit finding **H3**):

1. **An unrecognised stored commit silently becomes a self-rotation.**
   `recovered()` ends in `job.unwrap_or(Job::Rotate)`: any purpose it does not
   match is resumed as `Job::Rotate`. `purpose()` emits ~20 strings and
   `recovered()` matches 12; the gap is survived today only because the jobs that
   are missing from `recovered()` never leave a *pending* commit (they are handled
   directly in `run_job`, never through `send_commit`) — an invariant enforced in a
   *different* function, with nothing tying the two together. `Job::Rotate` itself
   has **no** arm in `recovered()`; it round-trips only because the fallback happens
   to be `Rotate`. A future commit-staging job with no `recovered()` arm would
   silently resume as a leaf rotation — a different membership change than the one
   the server already has pending.

2. **A chained follow-up is lost.** Removing a user stages a `RemoveFromGroup` whose
   `after` field carries the next job (`Admin.RemoveUser`, or the next group's
   remove). `purpose()` drops `after` and `recovered()` rebuilds it as `None`. A
   crash after the group-remove commit is staged but before the chain finishes
   would, on restart, complete the group-remove and **lose** the follow-up — leaving
   the user removed from named groups but never fully removed.

These are crash-recovery edge cases, not everyday paths, but they are silent and the
string format has already drifted (12 of 20 arms). The fix makes recovery **total**
and typed, so every commit-staging job round-trips and an unparseable record is
*cleared*, never reinterpreted.

## What Changes

- **Replace the string `purpose` codec with a typed, total serde record.** A new
  private `CommitPurpose` enum (in `client/src/engine/jobs.rs`) has one variant per
  job that can stage a commit, carries the exact fields `prepare()` re-reads to redo
  the commit, and is `serde`-serialised to the existing `outgoing_commit.purpose`
  `TEXT` column (JSON via `serde_json`, already a client dependency). The
  `RemoveFromGroup` variant carries `after` recursively, so the chain round-trips.
- **`Job::recovered()` returns `Option<Job>`.** It deserialises the stored record and
  maps it back to a `Job` (no reply, as today). An input it cannot parse yields
  `None` — never a silent `Job::Rotate`.
- **The restart guard clears an unrecognised commit** instead of resuming it. When a
  pending own commit's stored record does not parse into a known job, the core clears
  the pending commit and the row (the existing `clear_pending_in`) and logs it,
  rather than finishing some other job in its place.
- **A `debug_assert!` + test tie the two halves together.** Every job that can stage a
  commit must round-trip through `CommitPurpose`; a test enumerates the full
  commit-staging set and asserts each maps and parses back to an equal job, so adding
  a commit-staging job without a recovery arm fails the build/test.
- **Logging keeps a stable label.** `Job::fail()` logs a short static `label()` per
  job variant instead of the serialised record, so logs stay readable and are
  decoupled from the recovery codec.

### Out of scope

- The larger engine refactor unifying main-group and secondary-group storage (audit
  **H2**) — this change is deliberately confined to the recovery codec so it can land
  first and H2 rebases on it.
- Changing *which* jobs stage commits, the `send_commit`/`prepare`/`run_job`
  signatures, the `outgoing_commit` schema, or any wire/proto contract.

## Proto files touched

**None.** This is device-core internal state only (the local `outgoing_commit` row).
No `.proto` edit, so no `buf` / `pbgen` run.

## Invariants

No cryptographic or wire invariant changes. "Accepted means durable; nothing is
dropped silently" is respected: the cleared record is a *local, unsent-or-unmerged*
staged commit the server has not accepted, and clearing an **unparseable** one (a
should-never-happen corruption/format case) is strictly safer than resuming a
*different* membership change, which the current `unwrap_or(Job::Rotate)` does. The
new `client-runtime` requirement needs owner approval before apply.
