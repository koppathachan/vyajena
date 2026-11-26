# Design — harden-commit-recovery

## Context

`client/src/engine/jobs.rs` serialises an in-flight own commit's job to a string
(`Job::purpose()`), stored on the `outgoing_commit` row (`engine/mod.rs` `send_commit`),
and parses it back on restart (`Job::recovered()`, called from the restart guard in
`Engine::new`/init). The restart guard only reconstructs a job when the group still
holds a pending commit (`pending_commit().is_some()`); otherwise it clears the row.

## D1 — The real in-flight set (verification)

A commit row is written **only** by `send_commit`, which is reached **only** from
`run_job`'s `Prepared::Commit` branch. So the set of jobs that can leave a pending
commit is exactly the jobs whose `prepare()` arm returns `Prepared::Commit`:

```
AddUser, RemoveUser, RemoveDevice, Readd, AddNew, AddLinked, CommitServerRemoves,
Rotate, AddToDm, CommitServerRemovesIn, RotateIn, AddToGroup, RemoveFromGroup
```

(13 jobs.) The other job kinds — `CreateGroup`, `CreateNamedGroup`, `CloseGroup`,
`RemoveClosedDm`, `SetMutePref`, `UploadKeyPackages`, `RequestReadd`, `PurgeBlob`,
`OpenDm` — are handled directly in `run_job` and never call `send_commit` (their
`prepare()` arm is the `Prepared::Nothing` list), so they never persist a commit and
never need a recovery arm. This is why the current `recovered()` "survives" despite
missing arms — but nothing in the code ties that invariant down. The fix makes the
`CommitPurpose` enum *be* that set, and a test asserts the mapping is total.

Of the 13, the current codec is wrong in two spots:
- `Rotate` has no `recovered()` arm; it round-trips only by the `unwrap_or(Job::Rotate)`
  fallback coincidence.
- `RemoveFromGroup` drops its `after` continuation.

## D2 — Typed record, same column (chosen)

Introduce a private `CommitPurpose` serde enum in `jobs.rs` with one variant per
commit-staging job, carrying the fields `prepare()` re-reads:

```
AddUser { user }              RemoveUser { user }
RemoveDevice { device }       Readd { user, device, seen }
AddNew { user, device }       AddLinked { device }
CommitServerRemoves           Rotate
AddToDm { group_id, user, device }
CommitServerRemovesIn { group_id }
RotateIn { group_id }         AddToGroup { group_id, user }
RemoveFromGroup { group_id, user, after: Option<Box<CommitPurpose>> }
```

Serialise with `serde_json` (already a client dependency) into the existing
`outgoing_commit.purpose` `TEXT` column — no migration, no schema change, no proto.
`user` is `UserId`'s `Display`/`parse` string; `device`/`group_id`/`seen` are byte
vectors (serde encodes `Vec<u8>` as a JSON array — fine for this tiny, single-row
record; readability of the raw bytes is not a goal, correctness is).

Why serde JSON over prost: prost would need a `.proto` message + `pbgen` run, which
touches generated files and widens the merge surface with the H2 refactor. serde on
a private enum keeps the whole change inside `jobs.rs` + the restart guard.

Why not `#[derive(Serialize)]` on `Job` directly: `Job` carries a `Reply` (a oneshot
sender) and is not serialisable; a recovered job never has a reply anyway. The
dedicated enum carries only the recoverable fields.

`after` is `Option<Box<CommitPurpose>>`; today it only ever holds `RemoveUser`, but
modelling it as a full `CommitPurpose` keeps it total and future-proof and matches
`Job::RemoveFromGroup.after: Option<Box<Job>>`.

## D3 — Totality guard

- `Job::commit_purpose(&self) -> Option<CommitPurpose>`: exhaustive `match` over
  `Job`; the 13 commit-staging variants map to `Some(..)`, the rest to `None`. Because
  the `match` is exhaustive, adding a new `Job` variant forces a compile decision here.
- `Job::recovered(&str) -> Option<Job>`: `serde_json::from_str::<CommitPurpose>()` then
  map to `Job`. Unparseable or unknown → `None`.
- In `send_commit`, the stored string is `job.commit_purpose()` serialised;
  `debug_assert!(job.commit_purpose().is_some())` documents that only commit-staging
  jobs reach `send_commit`, and a `None` there (a should-never-happen) fails the
  staged commit cleanly rather than storing an empty record.
- A unit/integration test enumerates every commit-staging `Job`, asserts
  `commit_purpose().is_some()`, and asserts `recovered(serialised)` returns a job
  equal to the original (including `after`).

## D4 — Clearing an unrecognised record

The restart guard changes from:

```
let job = Job::recovered(&row.purpose);   // always a Job; unknown -> Rotate
self.inflight = Some(InFlight { row, job, .. });
```

to:

```
match Job::recovered(&row.purpose) {
    Some(job) => self.inflight = Some(InFlight { row, job, .. }),
    None => { warn!(...); self.clear_pending_in(&group_id)?; }  // drop staged commit + row
}
```

`clear_pending_in` already clears both the group's pending commit and the
`outgoing_commit` row in one transaction, for main group and DMs alike. Clearing is
safe: the commit was never accepted by the server; and this path is only reachable on
genuine record corruption/format drift, where resuming a *wrong* job (today's
behaviour) is strictly worse.

## D5 — Logging label

`Job::fail()` currently logs `purpose = %self.purpose()`. With `purpose()` gone,
`fail()` logs a short static `Job::label(&self) -> &'static str` (the variant name),
total over all jobs, decoupled from the recovery codec. `send_commit`'s log switches
from the serialised record to the same `label()`, so logs stay human-readable and
never depend on the storage format.

## Risks / H2 coordination

The change is confined to `jobs.rs` (the codec + `label()`) and the restart guard in
`engine/mod.rs`. It does not touch `prepare`, `run_job`, `finish`, or the
`send_commit`/store signatures beyond swapping the stored string's producer. If the
H2 refactor renames `Job` variants or splits group storage, the only overlap is the
1:1 `CommitPurpose` <-> `Job` arms, which mirror the existing `purpose()` arms — a
mechanical rebase. H3 lands first; H2 rebases on it.
