## 0. Before starting

- [ ] 0.1 Owner approves the reworded requirements (device-identity: Bootstrap admin;
  key-directory: Clients verify the identity chain) and answers OQ1 (operator
  passphrase?) and OQ2 (operator reminder line?) in design.md
- [ ] 0.2 Confirm with the tracker that `admin-reset`'s surgical reset stays separate
  from `server-deployment: Scripted reset` (design D4) and is documented under
  deploy/server/ ops, not folded into `deploy.sh --reset`

## 1. Proto and shared rules

- [ ] 1.1 No `.proto` change (design D7): the reset edits the server DB, re-onboarding
  reuses `Devices.Register`, the re-trust gate is a local client rule. Record "no wire
  change, no `PROTOCOL_VERSION` bump" and skip `pbgen`

## 2. Server — bootstrap gate

- [ ] 2.1 `ensure_bootstrap_invite` (`server/src/runtime/bootstrap.rs`): gate on "no
  **active** admin" instead of `users > 0` — re-mint when the admin user id is absent
  OR `removed_at IS NOT NULL`, and there is no usable bootstrap invite; return
  `NotNeeded` when the admin is active, whatever other users exist. Tests
  `device-identity/bootstrap-admin` (first-start, healthy-server-does-not-re-bootstrap,
  re-bootstrap-after-an-operator-admin-reset)
- [ ] 2.2 Regression: confirm `Devices.Register` for the admin after re-bootstrap takes
  the removed-user re-key path (`server/src/devices/mod.rs:420`) — new identity key,
  `removed_at` cleared, old admin devices stay revoked. Tests
  `admin-recovery/recovered-admin-re-bootstraps-with-a-fresh-identity`

## 3. Server — admin-reset subcommand

- [ ] 3.1 Add `Command::AdminReset { config }` to `command_from_args` and `USAGE`
  (`server/src/main.rs`), matching the `backup` / `egress-proxy` pattern; `Config::load`
  then open the configured database directly; refuse (error, no change) if the DB
  cannot be opened. Tests `admin-recovery/operator-break-glass-reset` (no-host-access)
- [ ] 3.2 Implement the reset transaction: set `users.removed_at = now` for
  `config.admin_username`, revoke the admin's devices, revoke the admin's unused
  invites; change nothing else (other users, groups, DMs, TLS/server keys, envelopes).
  Tests `admin-recovery/operator-break-glass-reset` (operator-runs-the-reset,
  blast-radius-is-one-user)
- [ ] 3.3 (OQ1, optional) Operator passphrase: optional `[recovery]`/field in config
  (`server-runtime: Configuration`) or interactive prompt; when set, a wrong/absent
  passphrase refuses the reset and changes nothing. Tests
  `admin-recovery/reset-needs-genuine-host-access-never-a-network-endpoint`
  (wrong-passphrase)
- [ ] 3.4 Assert there is no network path: a test/inventory check that no RPC performs
  admin reset. Tests `admin-recovery/reset-needs-genuine-host-access-never-a-network-endpoint`
  (no-rpc-can-reset-the-admin)

## 4. Core / client — re-trust gate

- [ ] 4.1 Where a changed pinned identity key is accepted (`client/src/keys/`,
  `client/src/membership/`): on any pinned-key change, pin the new key but mark the
  user **unverified / pending re-trust**; expose the change as an event the UI can act
  on. Do not change decryption. Tests `key-directory/clients-verify-the-identity-chain`
  (admin-re-adds-a-reset-user, back-from-holiday-after-a-reset,
  recovered-admin-s-key-must-be-acknowledged)
- [ ] 4.2 Trusted/verified state is withheld until an explicit acknowledgement API is
  called; no path auto-accepts a changed key as verified. Tests
  `admin-recovery/admin-re-key-forces-peer-re-trust` (no-silent-auto-accept)

## 5. Bridge

- [ ] 5.1 Expose the pending-re-trust state and an acknowledge action in
  `bridge/src/api/`; `cd app && flutter_rust_bridge_codegen generate`; bridge
  end-to-end test: admin re-key surfaces pending re-trust, acknowledge flips to trusted

## 6. App

- [ ] 6.1 Re-trust prompt UI (`app/lib/<feature>/`): on a pending-re-trust event show
  "⚠️ <user>'s safety number changed — re-trust?" with an acknowledge action; show the
  identity as unverified until acknowledged. Widget tests
  `app-ui/notices-in-the-chat` (safety-number-changed-prompt) or a new
  `app-ui/admin-screens` scenario as the tracker directs

## 7. Docs and ops

- [ ] 7.1 Document `server admin-reset` in deploy/server/ ops notes and
  docs/architecture-decisions.md: when to use it, that it needs host access, that it is
  NOT the whole-server reset, and that members will see a re-trust prompt

## 8. Verify

- [ ] 8.1 `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --check`, `cd app && flutter analyze`, `flutter test`,
  `openspec validate admin-recovery --strict`
- [ ] 8.2 Manual QA on a LOCAL throwaway server: lock the admin out (wipe its key
  store), run `admin-reset`, restart, re-register the admin from the re-minted bootstrap
  invite with a fresh identity key, confirm old admin devices stay locked out, and
  confirm a member device shows the admin re-trust prompt and withholds trust until
  acknowledged
