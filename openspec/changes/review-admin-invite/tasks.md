## 0. Before starting

- [x] 0.1 Owner approves the new and changed requirements (`device-identity:
  Reusable review admin invite`, `device-identity: Bootstrap admin`,
  `admin-recovery: Reset needs genuine host access, never a network endpoint`,
  `server-runtime: Configuration`, `server-deployment: Review admin invite
  hand-off`). Concurrency (last join wins) and PR 71 (close it) are decided
  (owner, 2026-11-27)

## 1. Proto

- [x] 1.1 Proto-first check (design D7): confirm no `.proto` change is needed
  (`InvitePayload` and `RegisterRequest` already carry everything). No pbgen run.
  If one turns out to be needed, stop and revise this change proto-first

## 2. Server: config and storage

- [x] 2.1 `[review] reusable_admin_invite` in `server/src/runtime/config.rs`
  (default false, unknown keys refused) and the startup `WARN`; tests in
  `server/tests/` tagged `server-runtime/configuration`
- [x] 2.2 Migration `0012_reusable_invites.sql`: `invites.reusable` column (design
  D1); `invites::ensure_reusable_admin` helper; `invites::revoke_unused_for` and
  `list_unused` skip reusable rows

## 3. Server: register and startup

- [x] 3.1 Move the revoke + reseed steps of `runtime::admin_reset::reset_admin`
  into one shared `take_over_admin` function; `admin-reset` calls it (existing
  admin-recovery tests stay green)
- [x] 3.2 `register()` reusable path (design D2): flag on → take over and re-key,
  never `used_at`; flag off → `PERMISSION_DENIED`. Revoked devices' streams
  close. Tests `server/tests/review_admin_invite.rs` tagged
  `device-identity/reusable-review-admin-invite` (every scenario)
- [x] 3.3 Startup: flag on → `ensure_reusable_admin_invite` and no bootstrap
  (design D3); test tagged `device-identity/bootstrap-admin`
- [x] 3.4 Test that without the flag no RPC re-seats the admin, and with it only
  the reusable code does; tagged
  `admin-recovery/reset-needs-genuine-host-access-never-a-network-endpoint`

## 4. Server: hand-off subcommand

- [x] 4.1 `server review-invite --config <toml>` in `server/src/main.rs` (design
  D4): prints the payload; refuses with the flag off or no database; logs no
  code. Tests tagged `server-deployment/review-admin-invite-hand-off`

## 5. Core end to end

- [x] 5.1 `bridge/tests/review_admin_invite.rs` (beside the bridge's
  admin-recovery end-to-end test, on the shared `client/tests/support` server):
  Alice registers with the reusable invite and creates the lobby; Bob registers
  with the same invite; Alice's core emits `Removed`; Bob creates the lobby; a
  member Carol is re-added and sees the admin re-trust prompt; a third join
  removes Bob. Tagged `device-identity/reusable-review-admin-invite`

## 6. Deploy

- [x] 6.1 `deploy/server/review.toml` sets `[review] reusable_admin_invite = true`;
  `server/tests/deploy_files.rs` asserts it, and that `config.toml` has no
  `[review]`
- [x] 6.2 `deploy/server/deploy.sh --review-invite` (0600 file, path printed,
  payload never printed); `deploy.sh` stops saving a review bootstrap invite
- [x] 6.3 `deploy/server/check.sh` fails `real review flag` if
  `/etc/vyajena/config.toml` has `[review]`
- [x] 6.4 `shellcheck deploy/server/*.sh` clean

## 7. Docs

- [x] 7.1 README "Review server": provision once, `--review-invite`, last join wins
- [x] 7.2 `app/store/listing.md`: reviewer note says the invite can be used any
  number of times; drop "works once" and the reset-before-each-review step
- [x] 7.3 `docs/architecture-decisions.md`: the review-server exception to the
  break-glass rule (design D6). `openspec/config.yaml` needs no change (its
  invariants do not name admin reset)

## 8. Finish

- [x] 8.1 `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --check`, `buf lint`, `openspec validate
  review-admin-invite --strict` all pass
- [ ] 8.2 Open the PR; close koppathachan/vyajena#71 with a comment linking it
  (owner chose this, 2026-11-27)
- [ ] 8.3 Owner's yes, then redeploy the review server with the flag on
  (`deploy/server/build.sh && deploy/server/deploy.sh --no-build`), run
  `deploy.sh --review-invite`, `check.sh` passes; owner pastes the invite into
  Play and App Store once
