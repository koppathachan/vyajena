# Design — replace-user (Phase 1)

## Context

"Replace user" is the admin workflow of removing a person and re-inviting them (same
or new username). Today the two halves are:

- `Client::remove_user` → per named group `RemoveFromGroup` (MLS Remove commit), then
  `Admin.RemoveUser` RPC (revoke devices, revoke invites, close DMs for the peer).
  `client/src/engine/jobs.rs:447-519`, `server/src/admin/mod.rs:227-280`.
- `Admin.CreateInvite` for the freed username; `Devices.Register` onboards.

The gap (shared note `remove-reinvite-bug-diagnosis-2026-11-24.md`): the removed
device is never told and never cleared, and the core cannot re-register while a local
account exists. So the removed person is stuck with a stale app that cannot accept the
new invite.

## Decisions

### D0. One change, not two; the admin screen is the separable slice.
Considered splitting a focused `user-removal-propagation` bug-fix change that
`replace-user` would build on. Chose to fold both into `replace-user` because the
feature cannot ship without the propagation fix and adds little on top (the baseline
already specs remove + re-invite under `group-membership: Removing a user`); a second
OpenSpec change to archive would be overhead for one coherent capability gap. The
tasks are ordered so the fix (proto → server → core → bridge → removed-device screen)
can be verified and shipped before the admin "Replace user" screen, which depends on
nothing else and can be deferred. Owner/tracker may overrule and split.

### D1. The removed device is told in-band with a new account-level frame — but the authoritative signal stays `PERMISSION_DENIED`.
A revoked device **cannot authenticate again** (`Devices.Authenticate` →
`PERMISSION_DENIED`, `server/src/devices/mod.rs:229`), so a durable "owed, re-offered
on every reconnect until acked" notice — the pattern used for `DmClosed` to the
*peer* — is not deliverable to the removed user's **own** devices: they never get
another authenticated stream. Therefore:

- **Online at removal:** the server delivers a new account-level removal frame on the
  device's live `Connect` stream, flushed **before** `revocations.send` tears it down
  (same transaction as the removal), so a connected device learns cleanly and with a
  reason.
- **Offline at removal:** on the device's next connect `Authenticate` answers
  `PERMISSION_DENIED`; the core already emits `Removed` on this (baseline
  `client-runtime: Connection lifecycle`, scenario "Revoked device"). This is the
  reliable fallback and needs no durable frame.

So the new frame is **best-effort** (improves the online UX) and carries no ack path;
the client-side reset + register-after-reset (D3) is what actually unblocks
re-onboarding in **both** cases. This corrects the earlier draft, which assumed a
durable owed-notice to the removed device — incoherent once its auth is revoked.

Alternative rejected: delay the auth-revoke until the device acks the frame, so an
offline device could receive it on a later authenticated connect. Rejected: it keeps
a removed device able to authenticate for an unbounded window — a security
regression against `device-identity: Device revocation`. Revoke immediately; rely on
`PERMISSION_DENIED` for the offline case.

### D2. Reset is explicit, not automatic.
The core must drop local state to re-register, but silently wiping a device's history
on `Removed` is destructive and surprising. `reset()` is a core API the UI calls only
after the user confirms. Default UX: `Removed` → terminal screen → "Start over" →
confirm erase → `reset()`. Matches the owner's earlier "reset clears profile" note
without taking the decision from the user. (Owner confirm: OQ1.)

### D3. `register()` stops being a dead end via reset, not a special re-register path.
Rather than teach `register()` to overwrite an existing account (risky: half-reset
states, key-store leftovers, the server's same-`device_id` guard at
`server/src/devices/mod.rs:52`), the device fully resets first and then registers as a
clean device with fresh keys. The server's removed-user reset path (`:423`) and
new-user path (`:412`) are unchanged; fresh keys mean a fresh `device_id`, so the
same-`device_id` guard is never hit.

### D4. DMs are cleared on both sides; the removed side is best-effort.
`close_dms_of` filters notices to `m.username != username` (peer only,
`server/src/dms/mod.rs:626`). Also deliver `DmClosed` to the removed user's own
**connected** devices (drop the filter for the live-stream delivery), so an online
removed device clears its DM list immediately. It is best-effort for the same reason
as D1 (own devices can't reconnect); after `reset()` the DM is gone regardless. The
peer path (owed, re-offered until acked) is unchanged. `DmClosed` itself does not
change on the wire.

Spec placement: the canonical home of "DM closed on user removal" is the
`direct-messages` capability, which lives in the still-unarchived `dms` change and so
is **not** in `openspec/specs/`. A `direct-messages` MODIFIED delta would fail
`openspec validate --strict`. The normative both-sides clause is therefore folded into
`group-membership: Removing a user` (the home of `Admin.RemoveUser`'s effects). When
`dms` is archived, the clause should move to a `direct-messages` MODIFIED delta and be
reconciled with `remove-closed-dm`.

### D5. No identity carry-over in Phase 1.
A replaced person gets a new identity key; members see "identity key changed" (the
existing reset semantics, `group-membership: Automatic add of new users`, scenario
"Reset user joins again"). Carrying identity across a rename without a wipe is Phase 2
(co-signed succession) and is out of scope.

### D6. Proto surface is one new `ServerFrame` variant; numbers TBD.
`ServerFrame.frame` currently uses fields 1–10 (`messaging.proto:55-71`). The new
account-level removal frame is a new variant (next free is 11) with a distinct
message name — **not** the existing `RemovedNotice` (`:247`), which is the per-group
inactivity re-add notice carrying `missed_since`. Working name: `AccountRemoved`
(final name/number TBD). No `ClientFrame` change (no ack, per D1). Final field numbers
MUST be reconciled at implementation with `remove-closed-dm`, `mute-notifications` and
`notification-reliability`, which also add `ServerFrame`/connect machinery; the tracker
coordinates collisions. If `remove-closed-dm`'s per-user connect marker can carry the
removal signal, this frame should be unified with it rather than added separately —
decide at implementation.

## Flow (replace on the same physical device)

```mermaid
sequenceDiagram
    participant Admin
    participant Server
    participant Bob as Bob's phone (removed)
    Admin->>Server: remove_user(bob)  [MLS Removes + Admin.RemoveUser]
    Server->>Server: revoke devices, revoke invites, close DMs (same txn)
    Server-->>Bob: AccountRemoved (+ DmClosed) if a stream is live  [best-effort]
    Server->>Bob: stream closed (revocation)
    Note over Bob: core emits Removed; history read-only; composer off
    Admin->>Server: create_invite(bob or robert)
    Admin-->>Bob: hands over the new invite
    Bob->>Bob: user taps "Start over", confirms -> reset() wipes account + keys
    Bob->>Server: Devices.Register(new invite, fresh keys)
    Server-->>Bob: onboarded (removed-user reset path :423, or new user :412)
    Admin->>Server: add_user / open_dm for the new identity
    Note over Bob: if Bob was OFFLINE at removal: next Authenticate -> PERMISSION_DENIED<br/>core emits Removed the same way; same Start over -> reset -> register
```

## Open questions for the owner

- OQ1: Confirm reset is user-driven (D2), not an auto-wipe on `Removed`.
- OQ2: On "Start over", wipe immediately on confirm, or offer a read-only grace /
  export first? (Phase 1 assumes immediate wipe on confirm.)
- OQ3: Protocol version: the new `ServerFrame` variant is backward-compatible (an old
  core ignores an unknown oneof field). Confirm no `PROTOCOL_VERSION` bump is needed,
  i.e. the server may send it unconditionally and old cores simply fall back to the
  `PERMISSION_DENIED` path. (If a bump is wanted, gate like `GroupClosed` does for
  v≥3.)

## Spikes

- S1: confirm the account-removal frame (and the own-side `DmClosed`) flushes on the
  live `Connect` stream **before** `revocations.send` closes it, in the same removal
  transaction, with no lost race. Result → here.
- S2: confirm `reset()` fully clears the OS key store (identity/auth/MLS/storage) and
  SQLite so a following `register` mints fresh keys and a fresh `device_id` (no
  `insert_device` collision). Prove on both phone key stores. Result → here.
