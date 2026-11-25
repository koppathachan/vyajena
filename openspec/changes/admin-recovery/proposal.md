## Why

The server pins each user's identity key and there is no higher authority than the
admin. If the admin's **only** device loses its key-store keys (device wipe,
reinstall, keychain cleared), the admin is **permanently locked out**:

- Identity is the keys in the OS key store; there is no logout and no password
  re-login (`client/src/keystore/mod.rs`). A wiped device holds no identity, auth,
  MLS or storage key, so it cannot authenticate or re-key as the admin.
- Only the admin can mint invites (`Admin.CreateInvite`), so no one else can invite
  the admin back.
- The bootstrap invite is minted **only when the users table is empty** —
  `ensure_bootstrap_invite` returns `NotNeeded` as soon as `users > 0`
  (`server/src/runtime/bootstrap.rs`). A live server already has users, so bootstrap
  never re-fires.

So a single-device admin whose keys are gone has no path back, and the whole
self-hosted deployment loses its only administrator. This is the admin-lockout risk
recorded in `auth-model-and-admin-lockout` (owner, 2026-11-25).

The decision (Akhil): **the admin OWNS the self-hosted server, so operator/host
access IS the recovery authority.** There is no need for cryptographic identity
recovery for this break-glass path (that is the deferred `trust-verification`
change). Whoever can reach the server's data directory on the box can already read
and destroy everything; letting them re-seat the admin adds no new trust assumption.

## What Changes

A **break-glass operator reset** that requires genuine server/host access — never a
network-reachable or unauthenticated endpoint — plus the client trust handling the
re-key forces.

- **New `server admin-reset` subcommand**, run locally on the box (same hand-rolled
  CLI as `backup` / `egress-proxy`, `server/src/main.rs`). It opens the server's own
  SQLite database directly, so the authority is filesystem access to the data
  directory (mode 0600, owned by the service user), optionally gated by an operator
  passphrase. It **marks the admin user removed** (sets `users.removed_at`) and
  revokes the admin's existing devices and unused invites. It touches nothing else:
  other users, groups, DMs, the TLS/server keys and all stored envelopes are left
  intact. It mints no invite itself.
- **Bootstrap re-fires for a removed admin.** `ensure_bootstrap_invite` today gates
  on "any users exist". It is changed to gate on "an **active** admin exists": with
  the admin marked removed and no usable bootstrap invite, the next start re-mints one
  bootstrap invite for the admin and prints it once — reusing the existing bootstrap
  path. A healthy server with an active admin still bootstraps nothing.
- **The admin re-registers with a fresh identity.** The admin onboards the new device
  from the re-minted bootstrap invite. `Devices.Register` takes the server's existing
  **removed-user re-key path** (`server/src/devices/mod.rs:420`): the admin row's
  `identity_public_key` is replaced with the new key and `removed_at` cleared; the old
  admin devices stay revoked. The admin regains Admin RPC authority.
- **Peers must re-trust the admin — acknowledge, never silent.** Because the admin
  re-keys, every member still has the admin's **old** identity key pinned. When a
  pinned identity key changes, the client now requires an explicit acknowledgement
  ("⚠️ <admin>'s safety number changed — re-trust?") before it treats that identity as
  trusted; it SHALL NEVER silently auto-accept the changed key as verified. This
  tightens `key-directory: Clients verify the identity chain` from a passive banner to
  an acknowledge-before-trust gate. This bare warning is the **minimum** trust handling
  for break-glass recovery.
- **Out of scope (deferred to `trust-verification`):** user-chosen recovery phrases
  that unwrap the identity key (so the admin keeps the same key and no re-trust is
  needed), quorum re-trust by signed member attestations, and verified badges. This
  change deliberately ships only operator-reset + the bare key-change acknowledgement.

Details and decisions: `design.md`.

## Open decisions resolved

- Admin recovery authority = **host/operator access**, not a cryptographic recovery
  key, for the break-glass path (owner, `auth-model-and-admin-lockout`, 2026-11-25).
- Reset mechanism = **local subcommand that touches the DB directly**, not a
  network-reachable RPC (no new attack surface; the authority is already the box).
- Re-key is accepted: the recovered admin gets a **new** identity key via the existing
  removed-user path; peers re-trust via the key-change acknowledgement, not silently.

## Open questions (owner)

- OQ1: Should `server admin-reset` additionally require an operator passphrase (set in
  the server config, per `server-runtime: Configuration`) on top of host access, or is
  filesystem access to the data directory sufficient? Design recommends the passphrase
  as optional defense-in-depth (D3).
- OQ2: After the reset, should the admin-reset command print a one-line reminder that
  members will see a safety-number-changed prompt, so the operator can warn them
  out-of-band? (Design D5 assumes yes — operator guidance only, no behaviour change.)

## Proto files touched

None. The reset is a local operator subcommand that edits the server's own database;
it adds no RPC, message or `ServerFrame` variant. Re-onboarding reuses the existing
`Devices.Register` wire path unchanged. The client re-trust gate is a local UX/trust
rule over already-specified `Keys.GetIdentityKeys` / commit processing.

## Invariants

No cryptographic invariant changes. The server still pins each user's identity key and
still verifies every device certificate against it. The admin's re-key is handled by
the existing removed-user reset path (old devices stay revoked). The only tightening is
client-side: a changed pinned identity key SHALL require explicit acknowledgement before
trust, never silent auto-accept. The reworded `device-identity: Bootstrap admin` and
`key-directory: Clients verify the identity chain` requirements need owner approval
before apply.
