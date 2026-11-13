## Why

The Pi server and the Rust core work and are tested, but nobody can chat yet. The
owner skipped the terminal client (2026-11-11) and wants the Flutter app next, so
the first hand-run chat happens on phones: two simulators on the owner's Mac
against a local Pi.

## What Changes

- New `app/`: the Flutter app for iOS and Android. It draws screens only and
  calls the Rust core through `flutter_rust_bridge`; it opens no connection.
- New `bridge/` crate in the Cargo workspace: thin glue that exposes
  `client::Client` and its event stream to Dart with plain types. Its generated
  Rust and Dart code is committed and regenerated locally by one command, never in
  CI (like `pb.rs`).
- Phone key stores in the core: iOS Keychain and Android Keystore implementations
  of the existing `KeyStore` trait (client-runtime: Key storage already requires
  them).
- Screens: join from an invite, chat with send status and notices, members,
  profile, link a device, the key-changed warning, and the admin screens (create
  the group, invites, remove a user, group settings).
- A very light Material 3 look, as the architecture doc already says:
  `ColorScheme.fromSeed` with a muted seed and the `neutral` variant, light and
  dark from the system. The July neon design handoff is not used.
- Push stays the core's placeholder token; no FCM or APNs.
- CI also runs `flutter analyze` and `flutter test` (no codegen in CI).
- **Proto:** `proto/vyajena/v1/messaging.proto` gains a `JoinNotice` server frame
  (non-breaking): the Pi tells the admin's devices a new user's device needs
  adding, and the admin's device adds it automatically (owner, 2026-11-12).

**Open decisions:**

- **#7 build order (owner, 2026-11-11):** the terminal client moves to later.
  New order: Pi server ✅, Rust core ✅, Flutter app (this change),
  `qa-verification`, relay + Pi deployment with real push, then the terminal
  client.
- **#3 desktop vs mobile:** iOS and Android only for now; desktop later (owner,
  2026-11-11).
- **Push:** real FCM / APNs come with the deployment change (owner, 2026-11-11).
- **#2 Q&A verification** moves to right after this change (owner, 2026-11-11).
- **Invariant clarification (owner-approved, 2026-11-11):** "plaintext never
  leaves the Rust core" cannot hold literally once a UI shows messages. New
  wording: decrypted text reaches Dart only to be shown; Dart never stores, logs
  or sends it. Keys never leave the core.

## Capabilities

### New Capabilities

- `app-ui`: what the Flutter app shows and lets each user do, how it uses the core,
  its look, and how it protects data on the phone.

### Modified Capabilities

- `group-membership`: adds automatic add of new users (owner, 2026-11-12: the
  admin inviting a user is the consent; no tap to add). The phone key stores
  implement the existing client-runtime: Key storage requirement.

## Impact

- **New code:** `app/` (Flutter, feature folders in `lib/`, tests in `test/` and
  `integration_test/`), `bridge/` (Rust, tests in `bridge/tests/`), iOS and Android
  key stores in `client/src/keystore/`.
- **Dependencies:** `flutter_rust_bridge` 2.12 (Rust and Dart), Flutter 3.44 /
  Dart 3.12; key store crates chosen by the spikes.
- **Docs:** `docs/architecture-decisions.md` (build order, #3 and push as decided,
  the invariant wording, Q&A after the app), `openspec/config.yaml` context,
  `CLAUDE.md` layout and commands, README local run guide (simulators, emulator).
- **CI:** `flutter analyze` and `flutter test` added; generated code is only
  checked, never produced.
