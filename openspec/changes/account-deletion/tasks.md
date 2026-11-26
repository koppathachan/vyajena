## 0. Before starting

- [ ] 0.1 Owner approves in-app account deletion as the B2 public-release blocker fix,
      and the immediate-delete (no grace period) and admin-disallowed decisions (D1,
      D4): pending owner
- [ ] 0.2 `replace-user` merged (provides `reset()` and the account-removal
      `ServerFrame` notice this change reuses) and `delete-messages` merged (provides
      `Blobs.DeleteBlob`); the trigger + UI do not need them, the full flow does

## 1. Proto and codegen

- [ ] 1.1 `proto/vyajena/v1/devices.proto`: add
      `rpc DeleteAccount(DeleteAccountRequest) returns (DeleteAccountResponse)` to the
      `Devices` service (authenticated, next to `RevokeDevice`); `DeleteAccountRequest`
      with **no** `user_id`; empty `DeleteAccountResponse`; doc comment points to
      account-deletion. `buf lint`, `buf breaking --against '.git#branch=main'`
- [ ] 1.2 `cargo run -p pbgen` to regenerate `server/src/pb.rs`,
      `server/tests/support/pb.rs`, `client/src/pb.rs`; commit the generated files with
      the proto change (CI never runs codegen)

## 2. Shared rules (`protocol/`)

- [ ] 2.1 `protocol`: the self-delete authorisation rule — the caller may delete only
      its own authenticated user, and the admin user is refused; tests
      `account-deletion/the-admin-cannot-self-delete-through-the-rpc`

## 3. Server

- [ ] 3.1 `Devices.DeleteAccount` handler: authorise (authenticated caller; reject the
      admin with `FAILED_PRECONDITION`); tests
      `account-deletion/user-deletes-their-own-account`,
      `account-deletion/the-admin-cannot-self-delete-through-the-rpc`
- [ ] 3.2 Run the removal in one transaction, reusing `Admin.RemoveUser`'s path: mark
      removed, revoke devices + unused invites; tests
      `account-deletion/user-deletes-their-own-account`
- [ ] 3.3 Evict from every group via server external-Remove proposals (reuse the
      inactivity issuer) and delete the user's queued entries; close every DM with
      `DmClosed` to peers (reuse `close_dms_of`); tests
      `account-deletion/deletion-evicts-the-user-from-every-group-and-closes-their-dms`
- [ ] 3.4 Delete key packages, push tokens and queued messages; purge the user's blobs
      (reuse `Blobs.DeleteBlob` semantics, no-op on absent); keep only `users.removed_at`
      + username; tests `account-deletion/what-the-server-deletes-and-what-it-retains`
- [ ] 3.5 Deliver the account-removal notice (reuse `replace-user`) to the user's other
      connected devices before the revoke closes their streams; tests
      `account-deletion/the-initiating-device-wipes-itself-on-success`

## 4. Core (`client/`)

- [ ] 4.1 `delete_account()` API: call `Devices.DeleteAccount`; on OK invoke the
      existing `reset()`; on error wipe nothing and surface the error; tests
      `account-deletion/the-initiating-device-wipes-itself-on-success`
- [ ] 4.2 A guard so an admin device does not call the RPC (uses admin awareness); gate
      availability on a server that advertises `DeleteAccount`; tests
      `account-deletion/the-admin-cannot-self-delete-through-the-rpc`

## 5. Bridge

- [ ] 5.1 Expose `delete_account()` through `bridge/src/api/`;
      `cd app && flutter_rust_bridge_codegen generate`; commit generated bindings;
      bridge end-to-end test against the real server

## 6. App

- [ ] 6.1 A "Delete account" entry in the Profile tab, gated on a server that advertises
      the RPC and hidden/disabled for the admin device; widget tests
      `app-ui/the-admin-sees-no-self-delete`
- [ ] 6.2 The irreversible-delete confirmation (names what is deleted; immediate, no
      undo); on OK wipe and route to onboarding, on failure keep the account and show
      the error; widget tests `app-ui/deleting-your-account`

## 7. Docs and check

- [ ] 7.1 Update `app/store/privacy.md` (remove "No in-app self-deletion yet"; state the
      in-app delete and what it deletes vs. retains) and the stores' Data-safety /
      5.1.1(v) answers; note B2 cleared in the submission notes
- [ ] 7.2 clippy (`--workspace --all-targets -D warnings`), fmt (`--check`), `buf lint`,
      `buf breaking`, `flutter analyze`, `flutter test`, `openspec validate
      account-deletion --strict` all pass; the change's own crate tests
      (protocol/server/client/bridge `account_deletion`) and widget tests pass. Full
      `cargo test --workspace` left to CI (slow external disk)
