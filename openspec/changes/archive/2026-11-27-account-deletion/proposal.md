## Why

A user has no way to delete their own account from inside the app. The only
deletion path today is **the admin removes a user** (group-membership: Removing a
user) — which the user cannot trigger themselves, and which Apple and Google both
reject as a substitute for self-service deletion.

- **Apple App Store Guideline 5.1.1(v)** requires an app that lets a user create an
  account to let that user **initiate deletion of the account from within the app**
  — not by email or "ask the admin", and a full delete (a grace period is allowed,
  mere deactivation is not).
- **Google Play's account-deletion / Data-safety policy** requires an app that
  creates accounts to offer an **in-app** path to request deletion of the account
  and its associated data (a web URL alone is not enough once there is an in-app
  account).

This is one of the two public-store-release blockers (the other, report/block, is
the separate `report-and-block` change). It is **B2** in the submission notes.
`app/store/privacy.md` already records it as a known gap ("No in-app self-deletion
yet: a public-release blocker"). The shipped `delete-messages` change is **message**
deletion, not account deletion.

The heavy lifting already exists: the server can revoke a user and free the
username (`Admin.RemoveUser`, group-membership: Removing a user); MLS eviction has a
server-issued external-Remove path that a remaining member/admin commits on next
connect (group-membership: Inactive devices are kept out); a device can wipe itself
to a clean no-account state (`reset()`, client-runtime: Device reset, change
`replace-user`); and a blob can be purged on demand (`Blobs.DeleteBlob`, change
`delete-messages`). This change only adds a **user-initiated trigger** for that same
removal and wires the in-app UI.

## What Changes

- **In-app "Delete account" action** (app-ui). A destructive entry in the Profile
  tab opens an explicit confirmation that names what is deleted and that it is
  **irreversible and immediate** (no deactivation, no cancellable grace period, no
  undo). Because it is irreversible the user must **type their own username** to
  confirm (the confirm control stays disabled until it matches); no single tap
  deletes the account.
- **A self-delete RPC** (proto-first): a new authenticated `Devices.DeleteAccount`
  that always targets **the caller's own user** (no `user_id` argument, so it can
  never delete anyone else), authorised by the caller's own device session token.
  On it the server runs the **same removal machinery as `Admin.RemoveUser`**, now
  triggered by the user:
  - marks the user removed (`users.removed_at`), **revokes all of the user's
    devices**, and revokes the user's unused invites (freeing the username);
  - **evicts the user from every group** (the lobby and every named group) by
    issuing server external-Remove proposals for all of the user's devices — the
    **existing inactivity external-Remove path** (group-membership: Inactive devices
    are kept out), committed by a remaining member or the admin on next connect — and
    deletes the user's queued entries in those groups;
  - **closes every DM** the user is in, delivering `DmClosed` to each peer (reuse
    `close_dms_of`);
  - **deletes the user's server-held data**: their uploaded key packages, their
    push tokens, and their queued/undelivered messages;
  - **purges the user's blobs**, reusing `Blobs.DeleteBlob` semantics (one stored
    copy per `blob_id`, keyed by the uploading device) to remove every blob the
    user's devices uploaded; purging an already-expired blob is a safe no-op;
  - delivers the **account-removal notice** (device-identity: Account-removal
    notice, change `replace-user`) to the user's **other** connected devices before
    the revoke tears their streams down, so they learn in-band and can reset too.
- **Local wipe on success** (local wipe). On an OK `DeleteAccount`, the initiating
  device calls the existing `reset()` (client-runtime: Device reset): it erases the
  local account, all groups, DMs and message history, and the device's identity,
  auth, MLS and storage keys from the OS key store, then returns the app to
  first-run / onboarding.
- **Admin self-deletion is disallowed** (the dangerous edge case). The admin account
  cannot be deleted through `DeleteAccount`: the server **rejects** the call when the
  caller's user is the admin, and the app hides/disables the action for the admin and
  explains why. The admin's only reset path stays the operator-only, host-gated
  `server admin-reset` (admin-recovery: Reset needs genuine host access), because
  bootstrap is possible only on an empty-users database, single-device admin
  key-loss is a permanent lockout, and the lobby's only committer is the admin. See
  `design.md` D4.
- **Out of scope:** a grace period / scheduled deletion (this change deletes
  immediately, which Apple and Google both accept); exporting data before deletion;
  deleting another user's account (that stays `Admin.RemoveUser`); admin succession
  or recovery (those are `admin-recovery`); report/block (`report-and-block`).

Details and decisions: `design.md`.

## Open decisions resolved

- **Immediate delete, not deactivation** (owner, 2026-11-26): no reversible
  "deactivate" state and **no cancellable grace period** (a grace period is allowed by
  the stores but not added here).
- **Confirmation friction: type-to-confirm** (owner, 2026-11-26): the user types their
  own username to confirm the irreversible delete (app-ui: Deleting your account).
- **Admin-disabled copy** (owner, 2026-11-26): the admin's entry is greyed-out with a
  short note pointing to the operator `server admin-reset` path (D4; app-ui: The admin
  sees no self-delete).
- **Admin self-deletion: disallowed**, routed to the operator `admin-reset` path
  (D4), not an RPC — this also upholds admin-recovery: "no network-reachable way to
  reset the admin".
- **MLS eviction mechanism: reuse the server external-Remove path** committed on
  next connect (the inactivity path), not a new self-commit flow (D3).
- **Local wipe: reuse `reset()`** from `replace-user`, user-confirmed, nothing
  silent (D5).

## Proto files touched

- `proto/vyajena/v1/devices.proto`: a new authenticated
  `rpc DeleteAccount(DeleteAccountRequest) returns (DeleteAccountResponse)`.
  `DeleteAccountRequest` carries **no** `user_id` (the server uses the caller's
  authenticated user), so the RPC can only ever delete the caller's own account;
  `DeleteAccountResponse` is empty. Placed in `Devices` next to `RevokeDevice` /
  `GetSelf` as a self-service account operation. Additive — `buf breaking` passes;
  regenerate with `cargo run -p pbgen` (CI does not run codegen; see design D6).
- **No new `ServerFrame` variant here.** The account-removal notice delivered to the
  user's other devices reuses the variant added by `replace-user` (device-identity:
  Account-removal notice); `DmClosed` and the external-Remove proposals already
  exist on the wire. This change depends on `replace-user` for that notice and for
  `reset()`.
- `protocol/`: the self-delete authorisation rule — the caller may delete only its
  own user, and never the admin.

## Capabilities

### New Capabilities

- `account-deletion`: the `Devices.DeleteAccount` RPC, what the server deletes vs.
  retains, the admin disallow rule, the eviction/close/purge reuse, and the
  on-success local wipe.

### Modified Capabilities

- `app-ui`: a "Delete account" entry in Profile, the irreversible-delete
  confirmation, the admin-disabled state, and the return to onboarding on success.

## Invariants

No cryptographic invariant changes. The change is additive and reuses existing
removal, revoke, close-DM, external-Remove and blob-purge machinery.

- **"Accepted means durable; nothing is dropped silently"** is respected: the
  user's **own** queued/undelivered copies are deleted as part of deleting their
  account at their own request, and peers keep whatever was already delivered to
  them (the server holds no history; E2EE cannot erase other members' received
  copies). `design.md` D2 states plainly what is and is not deleted.
- **admin-recovery: "no network-reachable way to reset the admin"** is upheld,
  because `DeleteAccount` refuses the admin (D4).

## Impact

- **Server (`server/`):** a `Devices.DeleteAccount` handler that authorises
  (caller's own user; reject the admin) and then runs the existing removal steps in
  one transaction — revoke devices + invites, mark removed, issue external Removes in
  each group and delete queued entries, close DMs, delete key packages / push tokens
  / queued messages, purge blobs. Almost all of this is the code behind
  `Admin.RemoveUser`, `close_dms_of`, the inactivity external-Remove issuer and
  `Blobs.DeleteBlob`, called from a self-service entry point.
- **Core (`client/`):** a `delete_account()` API that calls `DeleteAccount`, and on
  OK invokes the existing `reset()`; a guard so the admin's device does not offer it.
- **Bridge and app:** a "Delete account" item in Profile, the confirmation dialog,
  the admin-disabled state, and routing to onboarding after `reset()`.
- **Ops:** server deployed before the app update (an old server lacks
  `DeleteAccount`; the app gates the action on a server that advertises it, as other
  wire changes do). Proto regen (`pbgen`) is a local step at implementation; CI does
  not run it. `app/store/privacy.md` is updated to state in-app deletion exists.
- **Depends on:** `replace-user` (for `reset()` and the account-removal notice) and
  `delete-messages` (for `Blobs.DeleteBlob`); the trigger and UI stand alone.
