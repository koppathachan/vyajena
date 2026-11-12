## 1. Spikes

- [x] 1.1 Spike (git-ignored `spikes/`): an Android Keystore `KeyStore` from Rust on the emulator (a `keyring`-compatible Android store, else JNI sealing under a Keystore AES key) that survives an app restart; record the result under D4 and delete the spike
- [x] 1.2 Spike: `keyring` Apple-native store on the iOS Simulator (persistence, accessibility class); record under D4 and delete
- [x] 1.3 Spike: a one-function `flutter_rust_bridge` 2.12 crate with a tokio runtime and a `StreamSink`, built into a Flutter app on both simulators from the workspace; record under D2 and delete

## 2. Bridge

- [x] 2.1 `bridge/` crate in the workspace: plain types and functions forwarding to `client::Client`, one event `StreamSink`, text forms of invites, link requests and approvals (D2, D5)
- [x] 2.2 Generate and commit the Dart and Rust bindings; add the codegen command to `CLAUDE.md`
- [x] 2.3 `bridge/tests/` against a real in-process Pi: register, send, events in order, text codecs

## 3. Phone key stores

- [x] 3.1 iOS Keychain `KeyStore` (client-runtime: Key storage)
- [x] 3.2 Android Keystore `KeyStore` (client-runtime: Key storage)

## 4. App skeleton

- [x] 4.1 `flutter create` iOS + Android only in `app/`; feature folders; `CoreService` and its fake; the theme (app-ui: Light Material 3 look)
- [x] 4.2 Private storage, Android `allowBackup="false"`, iOS backup exclusion and `NSFileProtectionComplete` (app-ui: Data on the phone); lifecycle and connection indicator (app-ui: Foreground and background)

## 4b. Automatic add of new users (owner, 2026-11-12)

- [x] 4.3 Proto first: `JoinNotice { group_id, device }` as a new `ServerFrame` field in `messaging.proto` (non-breaking, `buf lint` / `buf breaking` clean); `cargo run -p pbgen`; commit the regenerated `pb.rs` files
- [x] 4.4 Pi: send `JoinNotice` to connected admin devices on registration and on each admin connect until the device is a member; `server/tests/` for the requirement's Pi side (group-membership: Automatic add of new users)
- [x] 4.5 Core: admin devices add on `JoinNotice` with no user action (KeyPackage for that device, first-sight pin, changed key waits for accept, race loser does nothing); bridge/app see it as the existing `JoinedGroup` / `MembersChanged` events; `client/tests/` for every scenario

## 5. Screens

- [x] 5.1 Join from an invite, with the server identity error (app-ui: Join from an invite)
- [x] 5.2 Chat with send status, composer limit, placeholders and notices (app-ui: Chat screen, Composer limit, Notices in the chat)
- [x] 5.3 Profile and members (app-ui: Profile and members)
- [x] 5.4 Admin screens: create group, invites, remove user, group name (app-ui: Admin screens)
- [x] 5.5 Link a device by text (app-ui: Link a device)

## 6. Tests and local run

- [ ] 6.1 Widget tests tagged `// spec: app-ui/<title-slug>` for every app-ui scenario, plus the import scan for "UI goes through the core"
- [ ] 6.2 `integration_test/`: the app on the iOS Simulator and on the Android emulator chats with a second member driven through the core, against a temp Pi (D6, D7)
- [ ] 6.3 README: run the app locally (simulator, emulator with `adb reverse`), never consuming the owner's invite in tests

## 7. CI, docs and validation

- [ ] 7.1 CI: `flutter analyze` and `flutter test` (no codegen)
- [ ] 7.2 `docs/architecture-decisions.md`: build order #7 (terminal client after deployment), #3 mobile now and desktop later, push with deployment, the invariant wording, Q&A after the app; the invariant in `openspec/config.yaml`; `CLAUDE.md` layout and commands; README status
- [ ] 7.3 `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, `buf lint`, `flutter analyze` clean; `openspec validate flutter-app --strict` passes
