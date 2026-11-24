# Tasks

Spec-only for now. The owner has resolved D1 (trust model = option A, already the
implementation) and D2 (no choose-server screen = option A2). Implementation is
gated on the shared external-disk cargo target being free (builds serialise).

## 1. Owner decisions (resolved)

- [x] 1.1 D1: trust via the invite's TLS pin (option A). Confirmed — already the
  implementation.
- [x] 1.2 D2: no pre-invite choose-server screen (option A2). Resolved by the owner.
- [ ] 1.3 D5 (relay topology: own vs shared relay) — surfaced for the owner in
  design.md; does not block this change. Answer feeds the operator-setup docs.

## 2. App — operator-neutral, invite-only onboarding (app-ui: Published build is operator-neutral in the UI)

- [ ] 2.1 Audit onboarding/link copy for any single implied server or group;
  neutralise to "the server you were invited to". Leave bundle id / display name
  alone.
- [ ] 2.2 Confirm onboarding goes straight to the invite with no choose-server
  step (A2). Keep the invite preview that shows the user id + relay address the
  invite names before Join.

## 3. Build guard (client-runtime: Published build is operator-agnostic)

- [ ] 3.1 Add a guard test asserting the built client/app embed no 32-byte TLS pin
  literal, no default `relay_address`, no `server_id` constant, and no embedded
  invite payload, and that onboarding copy names no hardcoded server host. Wire it
  into the client/app test suite.

## 4. Spec / docs

- [ ] 4.1 Fold the trust-anchor wording into `device-identity` "Invites" when the
  change is applied (the invite is the self-contained trust-and-config root).
- [ ] 4.2 Write an operator-setup doc section (README / `docs/`): the server TOML
  fields (`server_id`, `admin_username`, `listen_address`, `public_address`,
  `database_path`, `key_dir`), `[blob_store]` **required** for audio+images (no
  blob store ⇒ `Blobs` RPCs return `UNAVAILABLE`), and the first-start bootstrap
  invite. Reflect the D5 answer (own vs shared relay) once the owner gives it.
- [ ] 4.3 Note single-server-per-install / one-identity-per-install, and that
  multi-server/multi-account is out of scope (design D3).

## 5. Deferred (not in this change)

- [ ] 5.1 Optional `ServerInfo` (and/or invite) human-readable server display name
  — not needed for A2; proto field number **TBD** if the owner ever wants it.
- [ ] 5.2 Multi-server / multi-account in one install (design D3).

## 6. Validate

- [ ] 6.1 `openspec validate self-hostable --strict` passes.
