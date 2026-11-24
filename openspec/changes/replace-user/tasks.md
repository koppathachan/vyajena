## 0. Before starting

- [ ] 0.1 Owner approves the reworded requirements (group-membership: Removing a user;
  device-identity: Signup with an invite; client-runtime: Connection lifecycle) and
  answers OQ1–OQ3 in design.md
- [ ] 0.2 Reconcile the new `ServerFrame` field number and message name with the
  in-flight `remove-closed-dm`, `mute-notifications` and `notification-reliability`
  (tracker coordinates); decide whether to unify the removal signal with
  `remove-closed-dm`'s per-user connect marker (design D6). Do this AFTER
  `remove-closed-dm` has merged, as the tracker directs
- [ ] 0.3 Decide whether the `direct-messages` both-sides `DmClosed` clause stays folded
  in `group-membership: Removing a user` or moves to a `direct-messages` MODIFIED delta
  once `dms` is archived (design D4)

## 1. Proto and shared rules

- [ ] 1.1 `proto/vyajena/v1/messaging.proto`: add one `ServerFrame.frame` variant for
  the account-level removal notice (working name `AccountRemoved`), with its own
  message; keep it distinct from the per-group inactivity `RemovedNotice`. Use the
  reconciled field number (0.2). `buf lint`; `buf breaking --against '.git#branch=main'`;
  `cargo run -p pbgen`
- [ ] 1.2 `protocol`: per OQ3, document that an old core ignores the unknown frame and
  falls back to the `PERMISSION_DENIED` path; bump `PROTOCOL_VERSION` only if the owner
  asks. Tests `device-identity/account-removal-notice`

## 2. Spikes

- [ ] 2.1 S1: prove the account-removal frame and the own-side `DmClosed` flush on the
  live `Connect` stream before `revocations.send` closes it, in the same removal
  transaction, with no lost race. Result → design.md
- [ ] 2.2 S2: prove `reset()` fully clears the OS key store (identity/auth/MLS/storage)
  + SQLite so a following `register` mints fresh keys and a fresh `device_id` (no
  `insert_device` collision at `server/src/devices/mod.rs:52`). Prove on both phone key
  stores. Result → design.md

## 3. Server

- [ ] 3.1 `Admin.RemoveUser` (`server/src/admin/mod.rs:227-280`): in the removal
  transaction, queue/flush the account-removal frame to each of the removed user's own
  live streams before `revocations.send`; do not delay the revoke. Tests
  `group-membership/removing-a-user` (removed-device-is-told-in-band, removed-device-was-offline)
- [ ] 3.2 `dms::close_dms_of` (`server/src/dms/mod.rs:612-637`, filter `:626`): also
  deliver `DmClosed` to the removed user's own connected devices (best-effort on the
  live stream); leave the peer owed-notice path unchanged. Tests
  `group-membership/removing-a-user`
- [ ] 3.3 Confirm the removed-user reset (`register` `:423`) and new-username (`:412`)
  paths still onboard a reset device; regression test `group-membership/removing-a-user`
  (admin-re-invites-under-a-new-username)

## 4. Core

- [ ] 4.1 `Client::reset()` (`client/src/client.rs`): delete account, chats/DMs,
  messages, pending jobs and all key-store keys (identity, auth, MLS, storage); leave
  the device registerable; never auto-run. Tests `client-runtime/device-reset`
- [ ] 4.2 Engine (`client/src/engine/mod.rs:617`): on the account-removal frame emit
  `Event::Removed` once via the same path as the `PERMISSION_DENIED` handler; do NOT
  auto-reset. Tests `client-runtime/connection-lifecycle` (told-in-band)
- [ ] 4.3 `register()` (`client/src/client.rs:390`) unchanged except it now succeeds
  after `reset()`; end-to-end test: remove → (frame or `PERMISSION_DENIED`) → reset →
  register with a fresh invite → onboarded on the same physical device. Covers
  `device-identity/signup-with-an-invite` (re-onboarding-after-reset)

## 5. Bridge

- [ ] 5.1 Expose `reset()` in `bridge/src/api/`; map any new error kinds;
  `flutter_rust_bridge_codegen generate`; bridge end-to-end test for reset-then-register

## 6. App

- [ ] 6.1 Removed-device screen on `Event::Removed`: read-only history + "Start over"
  with an erase confirmation that calls `reset()` then routes to the invite screen.
  Widget tests `app-ui/removed-device-screen`
- [ ] 6.2 (Deferrable slice) Admin "Replace user" screen: remove a user then create a
  fresh invite (same or new username) in one flow. Widget tests `app-ui/admin-screens`
  (admin-replaces-a-user)

## 7. Verify

- [ ] 7.1 `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --check`, `buf lint`, `buf breaking`, `cargo run -p pbgen`
  (no diff), `flutter analyze`, `flutter test`, `openspec validate replace-user --strict`
- [ ] 7.2 Manual QA on a LOCAL throwaway server, both platforms: admin replaces a user
  (new username) end to end — removed, told (online) or `PERMISSION_DENIED` (offline),
  Start over → reset, re-onboarded with the fresh invite, re-added to the group, DM
  re-opened
