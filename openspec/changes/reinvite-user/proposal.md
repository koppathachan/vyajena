## Why

When a user **uninstalls the app** (or wipes its data) they lose the identity, auth,
MLS and storage keys the Rust core kept in the OS key store. Their username still
exists on the server with `removed_at IS NULL`, but the phone is now a keyless
stranger: a fresh install has no account, so there is no stuck-device problem — the
only thing missing is a way to let that reinstalled device sign up **again under the
same username with a new identity key**.

Today that takes **two admin actions**: `Admin.RemoveUser` (so the username's user row
becomes `removed`), then `Admin.CreateInvite`. Only after the remove does the server
allow it, because:

- `Admin.CreateInvite` rejects a present, non-removed username with `ALREADY_EXISTS`
  (`server/src/admin/mod.rs:76`).
- `Devices.Register` re-keys **only** in its removed-user branch
  (`server/src/devices/mod.rs:419-427`: `UPDATE users SET identity_public_key = ?,
  removed_at = NULL`); for a present user it returns `ALREADY_EXISTS`
  (`:434`).

The remove step is pure friction here: the person is not being removed, they are
coming back on the same name. It also widens the window of broken state (the account
is briefly `removed`, its DMs get closed, its invites revoked) for what should be a
single "re-invite this person" gesture.

The owner wants **one action**: an admin-minted invite for an **existing** username
should itself authorize a server-side identity re-key on join — no prior
`RemoveUser`. A fresh, single-use, username-bound admin invite **is** the
authorization.

This is the same primitive as the queued `replace-user` Phase 1 restricted to the
case "new username == old username", minus the remove and minus the stuck-device
reset `replace-user` needs. See `design.md` (D7) for the relationship and the
standalone-vs-fold recommendation. Root-cause background: shared note
`remove-reinvite-bug-diagnosis-2026-11-24.md`.

## What Changes

- **`Admin.CreateInvite` may be minted for a present, non-removed username.** It stops
  returning `ALREADY_EXISTS` for that case and instead issues an ordinary single-use
  invite bound to that username. Minting it **means** "re-invite / re-key this user".
  No new RPC, no new field: the existing `CreateInviteRequest { username }` is enough.
- **`Devices.Register` treats "valid unused invite whose bound username is a present
  user" as a re-key.** It updates that user's `identity_public_key`, keeps the **same
  `user_id`**, revokes **all** of the user's prior devices, and marks the new device,
  with **no prior `RemoveUser`**. The existing removed-user re-key path (`:419`) and
  new-user path (`:412`) are unchanged. An existing username presented **without** a
  valid fresh invite still cannot re-key (the invite is validated first, so an absent /
  used / expired / revoked invite is `PERMISSION_DENIED` as today).
- **The re-key kicks the user's working devices.** Because it revokes every prior
  device, re-inviting someone who still has a live phone logs that phone out (it will
  fail its next `Devices.Authenticate`). The admin app MUST warn and confirm before it
  calls `CreateInvite`.
- **Peers must re-trust.** The user's identity key changes, so every member's client
  shows the mandatory, non-silent "⚠️ `<name>`'s safety number changed — re-trust?"
  prompt and withholds the verified state until the owner acknowledges — the existing
  mechanism (`key-directory: Clients verify the identity chain`), nothing new and
  nothing auto-accepted.
- **No orphaned old leaf (admin-driven).** The re-key revokes the old devices; the
  reinstalled device rejoins the lobby and named groups through the normal `JoinNotice` →
  admin-Add → `Welcome` path (`group-membership: Automatic add of new users`), and in
  that same admin commit the admin device also Removes the re-keyed user's stale old
  leaves — automatically, no hand-picking, no separate `Admin.RemoveUser` and no new
  server eviction machinery (owner decision 2026-11-25, `design.md` D5).
- **App affordance:** on the admin members screen, a one-tap **"Re-invite `<name>`"**
  action (alongside the existing **Remove**) that calls `Admin.CreateInvite(sameUsername)`
  and shows the shareable code, behind a confirm dialog warning that it resets the
  person's devices and that they keep the same username but lose on-device history. The
  screen SHOULD show a per-user activity hint so the admin can spot who has likely lost
  access (`design.md` OQ4 / D8).

### Out of scope

- Carrying the old identity key across the reinstall (no-wipe succession) — that is the
  deferred `replace-user` Phase 2 / `trust-verification` territory.
- The stuck-device problem (a device that still **holds** its old account and must be
  reset before it can register) — that is `replace-user`'s `reset()` surface; a
  reinstalled device has no account, so it is not in scope here.
- Re-establishing the re-keyed user's **DMs** as a normative requirement. On re-key the
  old DM leaves go stale and the peer sees the changed identity key; full DM re-key /
  re-open behaviour is reconciled with the in-flight `remove-closed-dm` / `dms` work
  (`design.md` D5), not specced here.
- New-username replacement, in-place rename, blocking / muting.

## Open decisions resolved

- **A fresh single-use admin invite is the re-key authorization** (no separate "allow
  re-key" flag or RPC) — resolved: yes (D1).
- **Re-key with no prior `RemoveUser`** — resolved: yes; the present-user register
  branch re-keys directly (D2).
- **Re-key invalidates all of the user's existing devices, kicking any that survive** —
  resolved by the owner 2026-11-25: yes; gated by an admin confirm screen, no server-side
  refuse-if-active (D3).
- **Who evicts the old leaves** — resolved by the owner 2026-11-25: admin-driven, in the
  admin's re-add commit; no server-issued external Remove, no manual leaf-picking (D5).
- **Standalone `reinvite-user` vs. folding into `replace-user`** — resolved: standalone,
  sequenced to reconcile the shared register re-key branch (D7).

Owner still to confirm: OQ4 (the activity-hint scope on the members screen) in
`design.md`.

## Proto files touched

**None.** `Admin.CreateInvite` already takes a `username`
(`proto/vyajena/v1/admin.proto`) and `Devices.Register` already carries the invite
code, identity key and certificate; the whole change is server-side validation and
branch logic plus client UX. No `.proto` edit, so no `buf`/`pbgen` run (D6).

## Invariants

No cryptographic invariant changes. "The server is the authority for membership" and
"verify each leaf's device certificate against the user's pinned identity key" are
reinforced: a re-key is an admin-authorized (invite-gated) identity-key change that
members accept only through the existing `key-directory` re-trust path. The new
`device-identity` and `app-ui` requirements and the new `reinvite-user` capability need
owner approval before apply.
