## 0. Before starting

- [x] 0.1 Owner approves in-app account deletion as the B2 public-release blocker fix,
      and the immediate-delete (no grace period), type-to-confirm, admin-disabled-copy
      and admin-disallowed decisions (D1, D4): approved 2026-11-26
- [x] 0.2 `replace-user` (merged as `reinvite-user`, #37) and `delete-messages` merged
      (provides `Blobs.DeleteBlob`). NOTE: `reinvite-user` shipped a proto-free, admin-driven
      re-key and did **not** deliver a client `reset()` or an account-removal `ServerFrame`
      notice. This change therefore adds a local `reset()` itself (task 4.1) and relies on
      the existing `PERMISSION_DENIED` → `Removed` signal for the user's other devices
      instead of a new frame (design D7). Owner/tracker approved 2026-11-26.

## 1. Proto and codegen

- [x] 1.1 `proto/vyajena/v1/devices.proto`: add
      `rpc DeleteAccount(DeleteAccountRequest) returns (DeleteAccountResponse)` to the
      `Devices` service (authenticated, next to `RevokeDevice`); `DeleteAccountRequest`
      with **no** `user_id`; empty `DeleteAccountResponse`; doc comment points to
      account-deletion. `buf lint`, `buf breaking --against '.git#branch=main'`
- [x] 1.2 `cargo run -p pbgen` to regenerate `server/src/pb.rs`,
      `server/tests/support/pb.rs`, `client/src/pb.rs`; commit the generated files with
      the proto change (CI never runs codegen)

## 2. Shared rules (`protocol/`)

- [x] 2.1 `protocol`: the self-delete authorisation rule — the caller may delete only
      its own authenticated user, and the admin user is refused; tests
      `account-deletion/the-admin-cannot-self-delete-through-the-rpc`

## 3. Server

- [x] 3.1 `Devices.DeleteAccount` handler: authorise (authenticated caller; reject the
      admin with `FAILED_PRECONDITION`); tests
      `account-deletion/user-deletes-their-own-account`,
      `account-deletion/the-admin-cannot-self-delete-through-the-rpc`
- [x] 3.2 Run the removal in one transaction, reusing `Admin.RemoveUser`'s path: mark
      removed, revoke devices + unused invites; tests
      `account-deletion/user-deletes-their-own-account`
- [x] 3.3 Evict from every group via server external-Remove proposals (reuse the
      inactivity issuer) and delete the user's queued entries; close every DM with
      `DmClosed` to peers (reuse `close_dms_of`); tests
      `account-deletion/deletion-evicts-the-user-from-every-group-and-closes-their-dms`
- [x] 3.4 Delete key packages, push tokens and queued messages; purge the user's blobs
      (reuse `Blobs.DeleteBlob` semantics, no-op on absent); keep only `users.removed_at`
      + username; tests `account-deletion/what-the-server-deletes-and-what-it-retains`
- [x] 3.5 Other devices learn via the authoritative revocation signal — each revoked
      device's next `Devices.Authenticate` returns `PERMISSION_DENIED` → `Removed` (no new
      account-removal frame; `reinvite-user` never shipped one, see design D7); tests
      `account-deletion/the-initiating-device-wipes-itself-on-success`

## 4. Core (`client/`)

- [x] 4.1 `delete_account()` API: call `Devices.DeleteAccount`; on OK invoke the
      existing `reset()`; on error wipe nothing and surface the error; tests
      `account-deletion/the-initiating-device-wipes-itself-on-success`
- [x] 4.2 A guard so an admin device does not call the RPC (uses admin awareness); gate
      availability on a server that advertises `DeleteAccount`; tests
      `account-deletion/the-admin-cannot-self-delete-through-the-rpc`

## 5. Bridge

- [x] 5.1 Expose `delete_account()` through `bridge/src/api/` (a thin forward to
      `Client::delete_account`, no logic); `cd app && flutter_rust_bridge_codegen generate`;
      commit generated bindings. The forwarder's end-to-end behaviour is covered by the
      client `account_deletion` e2e (same pattern as `delete-messages`), not a separate
      bridge test

## 6. App

- [x] 6.1 A "Delete account" entry in the Profile tab (a too-old server returns
      `UNIMPLEMENTED`, surfaced as an error — no new capability flag, design D7); on the
      admin device greyed-out with the note pointing to `server admin-reset`; widget tests
      `app-ui/the-admin-sees-no-self-delete`
- [x] 6.2 The irreversible-delete confirmation (names what is deleted; immediate, no
      grace period, no undo) with **type-to-confirm** (confirm disabled until the typed
      text matches the user's username); on OK wipe and route to onboarding, on failure
      keep the account and show the error; widget tests `app-ui/deleting-your-account`

## 7. Docs and check

- [x] 7.1 Update `app/store/privacy.md` (remove "No in-app self-deletion yet"; state the
      in-app delete and what it deletes vs. retains) and the stores' Data-safety /
      5.1.1(v) answers; note B2 cleared in the submission notes
- [x] 7.2 clippy (`--workspace --all-targets -D warnings`), fmt (`--check`), `buf lint`,
      `buf breaking`, `flutter analyze`, `flutter test`, `openspec validate
      account-deletion --strict` all pass; the change's own crate tests
      (protocol/server/client/bridge `account_deletion`) and widget tests pass. Full
      `cargo test --workspace` left to CI (slow external disk)
