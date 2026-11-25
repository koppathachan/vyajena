## 0. Before starting

- [ ] 0.1 Owner approves the new requirements (`device-identity: Re-inviting an existing
  user`; `reinvite-user: Re-key evicts the user's old group leaves and rejoins through
  the normal path`; `reinvite-user: Re-invite changes the user's identity key for peers`;
  `app-ui: Re-invite action`) and answers OQ1–OQ3 in design.md
- [ ] 0.2 Reconcile with `replace-user`: decide which change lands the `register()`
  re-key structure first and rebase the other onto it (design D7). `reinvite-user` ADDs
  `device-identity: Re-inviting an existing user` and `app-ui: Re-invite action` so its
  deltas do not collide with `replace-user`'s `MODIFIED` "Signup with an invite" /
  "Admin screens"; confirm the two register re-key branches (removed-user and
  present-user-with-invite) are implemented coherently in one place
- [ ] 0.3 Confirm OQ2: `Admin.CreateInvite` for a present user is allowed
  unconditionally (gated by the UI confirm) vs. only when the user has no active device

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
- [ ] 2.2 S2: prove no orphaned leaf — the server's external Remove of each old leaf and
  the admin's combined Remove+Add (Welcome in the same `COMMIT`) leave the lobby and a
  named group with exactly the new device's leaf, with a member offline during the
  re-key catching up cleanly. Result → design.md

## 3. Server

- [ ] 3.1 `Admin.create_invite` (`server/src/admin/mod.rs:58-104`): allow a present,
  non-removed username (remove the `:76` `ALREADY_EXISTS` guard for that case; keep the
  "at most one usable invite per username" idempotence and the removed-user case). Tests
  `device-identity/re-inviting-an-existing-user` (admin-re-invites-a-reinstalled-user-on-the-same-name)
- [ ] 3.2 `register()` (`server/src/devices/mod.rs:384-440`): add a `Some(user) if
  !user.removed` branch that re-keys (`UPDATE users SET identity_public_key = ?,
  created_at = ?`), keeps the same `user_id`, and revokes every prior device via
  `auth::revoke_device` over `records::active_devices` (as `Admin.RemoveUser` does),
  before `insert_device` + mark-invite-used. Leave the `None` and `Some(removed)` paths
  unchanged, and keep invite validation and the cert `username@server_id` check ahead of
  the branch. Tests `device-identity/re-inviting-an-existing-user`
  (register-with-a-re-invite-re-keys-the-user, re-key-kicks-a-surviving-device,
  existing-user-without-a-valid-invite-cannot-re-key, re-invite-is-single-use)
- [ ] 3.3 Eviction: mark the re-keyed user's revoked old-device leaves for server-issued
  external Remove, reusing the revoked/inactive-device external-Remove path
  (`group-membership: Inactive devices are kept out`); reissued each epoch until
  committed. Tests `reinvite-user/re-key-evicts-the-user-s-old-group-leaves-and-rejoins-through-the-normal-path`
  (old-leaf-is-evicted-and-the-new-device-joins, member-offline-during-the-re-key-catches-up)

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

## 5. App (deferrable slice — design D8)

- [ ] 5.1 Add a one-tap "Re-invite `<name>`" action on the contact/member screen (admin
  devices only) that, after a confirm dialog warning it resets the person's devices and
  that they keep the username but lose on-device history, calls `Admin.CreateInvite` with
  the same username and shows the shareable invite text. No separate remove step. Widget
  tests `app-ui/re-invite-action` (admin-re-invites-a-member-who-lost-their-phone,
  confirming-shows-a-shareable-code, cancelling-changes-nothing, non-admin-sees-no-re-invite-action)

## 6. Verify

- [ ] 6.1 `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --check`, `flutter analyze`, `flutter test`, `openspec validate
  reinvite-user --strict`. No `buf`/`pbgen` step (no proto change, 1.1)
- [ ] 6.2 Manual QA on a LOCAL throwaway server, both platforms: admin re-invites a
  present user (same username), the reinstalled device onboards with a fresh identity
  key, any surviving old device is kicked (`PERMISSION_DENIED`), the old leaf is evicted
  and the new device rejoins the lobby + a named group, and every member sees the
  safety-number-changed re-trust prompt — all without a prior `Admin.RemoveUser`
