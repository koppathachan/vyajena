# Tasks

Spec-only for now. Implementation is gated on the owner confirming D1 (trust
model) and D2 (choose-server screen) in design.md, and on the shared external-disk
cargo target being free (builds serialise). Tasks below assume the recommended
path (D1 = option A, D2 = A1a).

## 1. Owner decisions (blocking)

- [ ] 1.1 Owner confirms D1: trust via the invite's TLS pin (option A). Already the
  implementation; confirm it is the intended model.
- [ ] 1.2 Owner confirms D2: keep the separate choose-server screen as a local
  pre-step (A1a), or choose A2 (no screen) / A1b (live preview).

## 2. App — choose-server step (app-ui: Choose a server)

- [ ] 2.1 Add a choose-server screen: a field for the public relay DNS (host, port
  defaulting to 443), validation, and store the result as the install's single
  server. No network call (A1a).
- [ ] 2.2 Persist the chosen server where device state already holds server fields
  (`client/src/store/device_state.rs` / its bridge surface); expose a getter to
  the app.
- [ ] 2.3 Route first-run so choose-server precedes the invite step; make the
  existing onboarding screen the second step. Link-a-device starts from the chosen
  server too.
- [ ] 2.4 Cross-check the invite against the chosen server: warn when the invite's
  `relay_address` or `server_id` differs, and block join until it matches or the
  server is re-chosen (app-ui: Join from an invite).

## 3. App — operator-neutral copy (app-ui: Published build is operator-neutral)

- [ ] 3.1 Neutralise onboarding/choose-server strings: no single implied server or
  group; "the server you were invited to". Leave bundle id / display name alone.

## 4. Build guard (client-runtime: Published build is operator-agnostic)

- [ ] 4.1 Add a guard test asserting the built client/app embed no 32-byte TLS pin
  literal, no default `relay_address`, no `server_id` constant, and no embedded
  invite payload. Wire it into the client/app test suite.

## 5. Spec / docs

- [ ] 5.1 Fold the trust-anchor wording into `device-identity` "Invites" and the
  new `client-runtime` / `app-ui` requirements when the change is applied.
- [ ] 5.2 Note single-server-per-install and the deferred multi-server nicety
  (design D3) in docs where onboarding is described.

## 6. Deferred (not in this change)

- [ ] 6.1 Optional `ServerInfo` (and/or invite) human-readable server display name
  so the choose-server screen can show a friendly name — proto field number **TBD**,
  reconciled with the tracker and the other in-flight proto changes at
  implementation.
- [ ] 6.2 Multi-server / multi-account in one install (design D3).

## 7. Validate

- [x] 7.1 `openspec validate self-hostable --strict` passes.
