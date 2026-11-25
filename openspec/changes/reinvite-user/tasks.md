## 0. Before starting

- [ ] 0.1 Owner approves the new requirements (`device-identity: Re-inviting an existing
  user`; `reinvite-user: Re-key evicts the user's old group leaves and rejoins through
  the normal path`; `reinvite-user: Re-invite changes the user's identity key for peers`;
  `app-ui: Re-invite action`). Re-key guard and eviction are resolved (owner 2026-11-25,
  design D3/D5); OQ4 (activity-hint scope on the members screen) still open
- [ ] 0.2 Reconcile with `replace-user`: decide which change lands the `register()`
  re-key structure first and rebase the other onto it (design D7). `reinvite-user` ADDs
  `device-identity: Re-inviting an existing user` and `app-ui: Re-invite action` so its
  deltas do not collide with `replace-user`'s `MODIFIED` "Signup with an invite" /
  "Admin screens"; confirm the two register re-key branches (removed-user and
  present-user-with-invite) are implemented coherently in one place

## 1. Proto and shared rules

- [ ] 1.1 Proto-first check (design D6): confirm **no** `.proto` change is needed —
  `CreateInviteRequest { username }` and `RegisterRequest` already carry everything. No
  `buf lint` / `buf breaking` / `cargo run -p pbgen` and no `PROTOCOL_VERSION` bump. If
  implementation finds a proto change is unavoidable, stop and revise this change
  proto-first

## 2. Spikes

- [ ] 2.1 S1: prove the present-user re-key branch in `register()` plus revoking the
  user's active devices in the **same transaction** leaves no window with two valid
  identity keys and no `insert_device` collision (fresh `device_id`,
  `server/src/devices/mod.rs:52`). Result → design.md
- [ ] 2.2 S2: prove no orphaned leaf — the admin's combined Remove(old leaf)+Add(new
  leaf) on the `JoinNotice` re-add (Welcome in the same `COMMIT`) leaves the lobby and a
  named group with exactly the new device's leaf, with a member offline during the re-key
  catching up cleanly; confirm the admin client recognises the stale leaves via the
  new pinned identity key without a separate `Admin.RemoveUser`. Result → design.md

## 3. Server

- [ ] 3.1 `Admin.create_invite` (`server/src/admin/mod.rs:58-104`): allow a present,
  non-removed username (drop the `ALREADY_EXISTS` guard for that case; keep the "at most
  one usable invite per username" idempotence and the removed-user case). Tests
  `device-identity/re-inviting-an-existing-user` (admin-can-re-invite-a-present-user)
- [ ] 3.2 `register()` (`server/src/devices/mod.rs:379-440`): add a `Some(user) if
  !user.removed` branch that re-keys (`UPDATE users SET identity_public_key = ?`),
  keeps the same `user_id`, and revokes every prior device via `auth::revoke_device`
  over `records::active_devices`; return the revoked device ids. The async `register`
  handler tears down those devices' live streams via `self.state.revocations.send`
  after commit. Leave the `None` and `Some(removed)` paths unchanged, and keep invite
  validation and the cert `username@server_id` check ahead of the branch. Tests
  `device-identity/re-inviting-an-existing-user` (re-invite-re-keys-the-user-and-revokes-prior-devices,
  a-re-invite-is-single-use, existing-user-without-a-valid-invite-cannot-re-key)
- [ ] 3.3 No new server eviction code (owner decision D5): eviction is admin-driven
  (task 4.3). Confirm the re-keyed new device qualifies for a `JoinNotice` after the
  re-key revokes the old devices (the `WAITING` query's "user has no non-revoked member
  device" clause is satisfied); regression test `group-membership/automatic-add-of-new-users`

## 4. Core / client

- [ ] 4.1 Confirm no core change is needed for the reinstalled device: a fresh install
  has no account, so `register()` runs as a new signup against the server's re-key
  branch; `register()` and the key store are unchanged. Add an end-to-end core test:
  reinstall (no account) → register with a re-invite → onboarded as the same user id
  with a fresh identity key. Covers `device-identity/re-inviting-an-existing-user`
- [ ] 4.2 Confirm the peer side needs no new code: a re-keyed user is a changed identity
  key handled by the existing `key-directory: Clients verify the identity chain` path
  (design D4). Add/extend a test that a re-invited user triggers the non-silent re-trust
  prompt and that a non-admin add of the re-keyed device is rejected. Tests
  `reinvite-user/re-invite-changes-the-user-s-identity-key-for-peers`
  (members-see-a-re-trust-prompt-after-a-re-invite, non-admin-cannot-slip-in-the-re-keyed-device)
- [ ] 4.3 Admin-driven eviction (`client/src/engine/`): when the admin device processes
  a `JoinNotice` for a device whose user it already has leaves for under a now-changed
  identity key, it SHALL commit the Add of the new device **together with** the Remove of
  that user's stale leaves (those not verifying against the new pinned key), Welcome in
  the same `COMMIT` — reusing the existing per-user Remove path (`engine/jobs.rs`), with
  no separate `Admin.RemoveUser`. Tests `reinvite-user/re-key-evicts-the-user-s-old-group-leaves-and-rejoins-through-the-normal-path`
  (old-leaf-is-evicted-and-the-new-device-joins, member-offline-during-the-re-key-catches-up)

## 5. App (design D8)

- [ ] 5.1 Admin members screen: add a one-tap "Re-invite `<name>`" action (admin devices
  only) alongside Remove that, after a confirm dialog warning it resets the person's
  devices and that they keep the username but lose on-device history, calls
  `Admin.CreateInvite` with the same username and shows the shareable invite text. Widget
  tests `app-ui/re-invite-action` (admin-re-invites-a-member-who-lost-their-phone,
  confirming-shows-a-shareable-code, cancelling-changes-nothing, non-admin-sees-no-re-invite-action)
- [ ] 5.2 (OQ4, scope TBC) Per-user activity hint on the members screen ("no active
  device" / last-seen). If it needs a new admin read RPC, raise it proto-first or defer
  to a follow-up per the owner's answer to OQ4

## 6. Verify

- [ ] 6.1 `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --check`, `flutter analyze`, `flutter test`, `openspec validate
  reinvite-user --strict`. No `buf`/`pbgen` step (no proto change, 1.1). Run only on an
  exclusive build slot (shared external-disk cargo target)
- [ ] 6.2 Manual QA on a LOCAL throwaway server, both platforms: admin re-invites a
  present user (same username), the reinstalled device onboards with a fresh identity
  key, any surviving old device is kicked (`PERMISSION_DENIED`), the admin's re-add
  removes the old leaf and the new device rejoins the lobby + a named group, and every
  member sees the safety-number-changed re-trust prompt — all without a prior
  `Admin.RemoveUser`
