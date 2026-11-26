# Tasks — dev-onboarding

Staged so the server + core are testable headless (stages 1–5) before the app and
the live simulator (stages 6–8). Proto first.

## 1. Proto (proto-first)
- [x] 1.1 In `proto/vyajena/v1/devices.proto`, add `string join_code = 4` to
      `RegisterRequest` (doc: empty in open mode; the shared secret when required;
      never logged).
- [x] 1.2 Add a nested `message Onboarding { enum Mode { MODE_UNSPECIFIED = 0;
      MODE_INVITE = 1; MODE_OPEN = 2; } ... }` and `Onboarding onboarding = 5` to
      `ServerInfo`. `MODE_UNSPECIFIED = 0` (buf STANDARD) reads as invite mode; the
      server always sets an explicit mode.
- [x] 1.3 `buf lint` and `buf breaking --against '.git#branch=main'` clean.
- [x] 1.4 Regenerate committed stubs: `cargo run -p pbgen`; commit `server/src/pb.rs`,
      `server/tests/support/pb.rs`, `client/src/pb.rs` with the proto change.

## 2. Config schema (server)
- [x] 2.1 Add an `Onboarding` config struct to `server/src/runtime/config.rs`
      (`mode: OnboardingMode` default `Invite`, `join_code: Option<String>`,
      `max_users: Option<u32>`), parse `[onboarding]`, add its fields to the
      unknown-field allow-list, validate (`max_users` > 0; `mode` one of the two),
      refuse to start naming a bad field.
- [x] 2.2 On `mode = "open"`, log a WARNING once at startup that open onboarding is
      enabled. Ensure `join_code` is never logged (extend the startup-config log /
      redaction).
- [x] 2.3 Document `[onboarding]` (mode, join_code, max_users; default invite-only)
      in the `config.rs` doc comment and in `server/local.toml` as commented
      examples, warning it is dev/demo only.

## 3. Register path (server)
- [x] 3.1 In `server/src/devices/mod.rs` `register`, branch on an empty
      `invite_code`: with a code, today's invite path unchanged; empty + invite mode
      → `FAILED_PRECONDITION`; empty + open mode → open path.
- [x] 3.2 Open path: verify the certificate against the identity key, take the
      username from the verified cert, check `join_code` if configured
      (`PERMISSION_DENIED` on mismatch), enforce `max_users` on a **new** user
      (`RESOURCE_EXHAUSTED`), reject a username already held by an active user
      (`ALREADY_EXISTS`), then create user+device atomically. Admin username →
      admin, via the existing `is_admin`/`admin_username` derivation (no new field).
- [x] 3.3 Enforce `max_users` on the invite path too (new user only); leave re-key,
      removed-user re-signup and `link` uncounted.

## 4. ServerInfo advertisement (server)
- [x] 4.1 Populate `ServerInfo.onboarding` in the `GetServerInfo` handler from the
      effective config (`mode`, `join_code_required = join_code.is_some()`,
      `max_users` or 0, `admin_username`).

## 5. Core open registration (client)
- [x] 5.1 Add `register_open(relay_address, tls_pin, username, join_code)` to the
      core (mirror `register` in `client/src/client.rs`): generate identity + device
      keys in the key store, build the cert for `username@server_id`, call
      `Devices.Register` with empty invite code + join code. Keys never leave the
      core. (Also added `server_onboarding` so the app can read the advertisement.)
- [x] 5.2 Map the server's `FAILED_PRECONDITION` to a distinct
      `OpenOnboardingUnavailable` error; no account is saved for an uncreated user.

## 6. Server + core tests
- [x] 6.1 Invite mode unchanged: invite-less register → `FAILED_PRECONDITION`;
      invited signup still works. // spec: device-identity/signup-with-an-invite
- [x] 6.2 Open signup without an invite creates the user + device; the device then
      authenticates. // spec: device-identity/open-registration
- [x] 6.3 Join code: correct accepts, wrong → `PERMISSION_DENIED`, none-configured
      accepts empty. // spec: device-identity/join-code
- [x] 6.4 Default-admin auto-claim: open-registering the admin username yields an
      admin; another user is not admin. // spec: device-identity/default-admin-auto-claim
- [x] 6.5 `max_users`: cap reached → `RESOURCE_EXHAUSTED`; link under the cap
      accepted. // spec: device-identity/maximum-users
- [x] 6.6 `[onboarding]` parsing: default is invite-only; open logs a WARNING;
      `join_code` never logged; unknown field refuses start.
      // spec: server-runtime/dev-onboarding-mode
- [x] 6.7 `ServerInfo.onboarding` advertises the effective mode/flags.
      // spec: server-runtime/server-advertises-its-onboarding-mode
- [x] 6.8 Core `register_open` end-to-end against a real open-mode test server; and
      the invite-mode error. // spec: client-runtime/open-registration-in-the-core

## 7. App (Flutter)
- [ ] 7.1 Regenerate the bridge after the core API change: in `app/`,
      `flutter_rust_bridge_codegen generate`; commit `bridge/src/frb_generated.rs`
      and `app/lib/src/rust/`.
- [ ] 7.2 In onboarding, read `ServerInfo.onboarding`; when `OPEN`, show the
      username (+ join code if required) screen and the "join as the admin" path,
      calling `register_open`; when `INVITE`, keep the paste-an-invite screen.
- [ ] 7.3 Widget test on the fake core: open mode shows the simplified screen and
      "join as admin"; invite mode shows paste-invite.
      // spec: app-ui/simplified-onboarding-in-open-mode

## 8. Live demo + docs
- [ ] 8.1 Stand up a throwaway open-mode local server in a temp dir (run.sh
      pattern; never `server/local.toml`, never the real/review server) and run the
      app on a dedicated iOS simulator through the simplified onboarding into the
      lobby.
- [ ] 8.2 Note the dev/demo onboarding mode in `docs/architecture-decisions.md`
      (config-gated, prod-off, E2EE-preserving; the password-vs-escrow decision D2
      and the admin-only MLS-add limit D5).

## 9. Done
- [ ] 9.1 `cargo test --workspace`, `cargo clippy --workspace --all-targets -D
      warnings`, `cargo fmt --check`, `buf lint`, `cd app && flutter analyze &&
      flutter test`, and `openspec validate dev-onboarding --strict` all pass.
