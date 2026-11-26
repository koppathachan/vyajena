## 0. Before starting

- [ ] 0.1 Owner approves the new requirement (`client-runtime: Resume a staged own
  commit across a restart`). It codifies existing behaviour and closes a silent
  crash-recovery gap; no cryptographic or wire invariant changes (proposal
  "Invariants").

## 1. Proto and shared rules

- [x] 1.1 Proto-first check: confirm **no** `.proto` change — this is device-core
  local state (`outgoing_commit.purpose`) only. No `buf` / `pbgen`, no
  `PROTOCOL_VERSION` bump.

## 2. Typed recovery codec (`client/src/engine/jobs.rs`)

- [x] 2.1 Add a private `CommitPurpose` serde enum with one variant per
  commit-staging job (the 13 in design D1), carrying the fields `prepare()` re-reads;
  `RemoveFromGroup { .., after: Option<Box<CommitPurpose>> }`.
- [x] 2.2 Add `Job::commit_purpose(&self) -> Option<CommitPurpose>`: exhaustive match,
  `Some(..)` for the 13 commit-staging jobs, `None` for the rest. (Plus
  `Job::stored_purpose() -> String` as the JSON serialiser.)
- [x] 2.3 Change `Job::recovered(purpose: &str) -> Option<Job>`: `serde_json::from_str`
  into `CommitPurpose`, map back to a reply-less `Job` (incl. `after`); unknown /
  unparseable → `None`. Remove the `unwrap_or(Job::Rotate)` fallback.
- [x] 2.4 Replace `Job::purpose()`'s string producer (now `stored_purpose()`): the
  stored string is `serde_json::to_string(&commit_purpose)`. Add
  `Job::label(&self) -> &'static str` (variant name) for logging and switch
  `Job::fail()` to it.

## 3. Restart guard (`client/src/engine/mod.rs`)

- [x] 3.1 In `send_commit`, store `job.stored_purpose()` and
  `debug_assert!(job.commit_purpose().is_some())`; log `job.label()`.
- [x] 3.2 In the restart recovery guard, match `Job::recovered(&row.purpose)`:
  `Some(job)` → set `inflight`; `None` → warn and `clear_pending_in(&group_id)` (drop
  the staged commit and the row) instead of resuming it.

## 4. Crash-recovery tests (`client/tests/commit_recovery.rs`)

Tests live in `client/tests/` only (CLAUDE.md: never in source files), so they reach
only the public API — the codec is exercised end to end through the real
persistence/restart path, not as an in-crate unit round-trip.

- [x] 4.1 Totality of the `Job` → `CommitPurpose` direction is enforced at compile
  time by the exhaustive match in `commit_purpose()`, plus the `debug_assert!` in
  `send_commit`. The reverse direction (round-trip) is covered behaviourally by 4.3 and
  by the existing `group_membership::a_commit_in_flight_survives_a_crash…` (now routed
  through the typed codec for `AddUser`).
- [x] 4.2 `Rotate` is a unit variant (no fields) and round-trips trivially; it is
  covered by the compile-time exhaustiveness rather than a dedicated time-driven crash
  test. (Its previous recovery relied on the `unwrap_or(Job::Rotate)` coincidence,
  which is now removed in favour of an explicit arm.)
- [x] 4.3 `a_group_removal_keeps_its_follow_up_across_a_crash`: a named-group user
  removal, crashed while its commit (carrying `after = Admin.RemoveUser`) is in flight,
  finishes the removal **and** runs the chained `Admin.RemoveUser` on restart (asserted
  via the device being revoked server-side). Verified to fail when `after` is dropped.
- [x] 4.4 `an_unreadable_stored_commit_is_cleared_not_reinterpreted`: a pending own
  commit with a corrupted stored record is cleared on restart (row gone, epoch
  unchanged, target user still a member), not resumed as a rotation.

## 5. Definition of done

- [x] 5.1 `cargo build --workspace` clean; `client` clippy (`--all-targets -D
  warnings`) and the recovery suites (`commit_recovery`, `group_membership`,
  `resume_across_restart`, `named_groups`) pass; the three changed files are
  `cargo fmt`-clean. Isolated `CARGO_TARGET_DIR` per the brief.
- [x] 5.2 `openspec validate harden-commit-recovery --strict` passes.
- [x] 5.3 AUDIT.md H3 updated with the resolution; tasks ticked.
