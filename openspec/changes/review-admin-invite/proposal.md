## Why

Store reviewers (Apple, Google) join the review server as its admin with the
bootstrap invite, which works once and expires in 7 days. Every new review, a
reinstall or a second reviewer needs the owner to run `deploy.sh --reset-review`
and paste a fresh invite into the store forms. The owner wants to provision the
review admin once, hand off one invite, and let reviewers join and re-join as
often as they like (owner, 2026-11-27).

PR koppathachan/vyajena#71 (open-mode onboarding) tried to solve this and does
not: its "Join as the admin" fails with `ALREADY_EXISTS` once the admin exists,
and without an invite the app cannot learn the server's address and TLS pin.
That PR is closed in favour of this change.

## What Changes

- **Reusable review admin invite.** A server whose config sets
  `[review] reusable_admin_invite = true` mints one admin invite that never
  expires and is never used up. It is an ordinary `InvitePayload` (address, TLS
  pin, code, admin user id), so the app pastes it like any invite.
- **Each use takes over the admin seat (last join wins).** In one transaction
  the server revokes the admin's existing devices, re-seeds the system lobby
  (the same steps as `server admin-reset`), re-keys the admin to the new
  device's identity key and registers the device. The reviewer who was there
  before is revoked and sees "removed".
- **No bootstrap invite** on such a server: the reusable invite replaces it.
- **Hand-off command.** `server review-invite --config <file>` (host access
  only) prints the reusable invite. `deploy/server/deploy.sh --review-invite`
  runs it on the review server and saves it to the owner-only
  `~/vyajena-server-backups/review-admin-invite-<UTC>.txt`.
- **Guard rails.** The real server's config never sets `[review]`;
  `deploy/server/check.sh` fails if it does. The server logs a startup `WARN`
  when the flag is on.
- **Spec carve-out (owner-approved, 2026-11-27):** `admin-recovery` says no
  network path may re-seat the admin. A server with the review flag on is the
  one exception; the flag is the operator's consent.
- No `.proto` changes. No app or core changes.

Open decisions resolved (owner, 2026-11-27): one invite and last join wins, not
one invite per store; PR 71 is closed, not reworked.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `device-identity`: new requirement "Reusable review admin invite"; "Bootstrap
  admin" skips the bootstrap invite when the review flag is on.
- `admin-recovery`: "Reset needs genuine host access, never a network endpoint"
  gains the review-server exception.
- `server-runtime`: "Configuration" gains the optional `[review]` table.
- `server-deployment`: new requirement "Review admin invite hand-off" (the
  deploy command and the real-server guard in the hardening check).

## Impact

- **proto:** none.
- **server:** `server/src/store/migrations/` (an `invites.reusable` column),
  `server/src/runtime/{config,bootstrap,admin_reset,mod}.rs`,
  `server/src/devices/mod.rs` (register), `server/src/main.rs` (subcommand),
  `server/src/runtime/review_invite.rs`, new tests `server/tests/review_admin_invite.rs`.
- **client / bridge:** an end-to-end test only (`bridge/tests/review_admin_invite.rs`)
  and a `start_review()` helper in `client/tests/support/server.rs`.
- **deploy:** `deploy/server/review.toml`, `deploy/server/deploy.sh`,
  `deploy/server/check.sh`, `server/tests/deploy_files.rs`.
- **docs:** README "Review server", `app/store/listing.md`,
  `docs/architecture-decisions.md`.
- **Review server:** redeploy with the flag on (owner's yes first); the old
  single-use bootstrap flow goes away there.
- **Not changed:** the real server, its data, its pin, its onboarding.
