## 1. Pi server

- [x] 1.1 `Devices.Authenticate`: after a valid signature, a revoked device gets `PERMISSION_DENIED`; a bad signature stays `UNAUTHENTICATED`; `server/tests/` (wire-transport: Session authentication)
- [x] 1.2 Revoking a device closes its open `Connect` stream with `PERMISSION_DENIED`; `server/tests/` (device-identity: Device revocation)
- [x] 1.3 The Pi logs `revoked an invite` (kind and ids only) (D8)

## 2. Core

- [x] 2.1 `Removed` event: on `PERMISSION_DENIED` from `Authenticate` or the stream, emit it once and stop connecting; history stays readable; `client/tests/` (client-runtime: Connection lifecycle)
- [x] 2.2 `reconnect_now()`: skips the backoff wait when disconnected, no-op when connected; `client/tests/` (client-runtime: Connection lifecycle)
- [x] 2.3 `MembersChanged { added, removed, by }` and `GroupSettingsChanged { name, by }`; inactivity Removes are marked so the UI can stay quiet (D5); `client/tests/` (client-runtime: Events in order)
- [x] 2.4 After merging a commit that adds devices: admin devices re-send `GroupSettings` if a name is set, every device re-sends its own user's `Profile` if set (D2); `client/tests/`: a linked tablet and a newly added user both get the name and profiles (message-content: Group settings travel encrypted; User-owned profiles)
- [x] 2.5 `IdentityKeyChanged` drops the user's stored profile (D3); `client/tests/` (message-content: User-owned profiles)

## 3. Bridge

- [x] 3.1 `bridge/src/api/`: `own_user` on `ChatMessage`, `Removed`, new event fields, `reconnect_now`; `bridge/tests/`; regenerate with `cd app && flutter_rust_bridge_codegen generate`

## 4. App

- [x] 4.1 Message tile: own-user messages right-aligned, sized to text, no label; status inline for `outgoing` only (D1); widget tests (app-ui: Chat screen)
- [x] 4.2 Notices: removed (indicator removed, composer disabled), rename, joined / added a device / removed a device / removed a user, quiet for inactivity re-adds and own join (D5); widget tests (app-ui: Notices in the chat)
- [x] 4.3 `connectivity_plus` listener calls `reconnect_now` on a network change (D6); widget test on the fake core (app-ui: Foreground and background)

## 5. Pi

- [x] 5.1 `check.sh`: `kernel-reboot` warning (D7); `shellcheck` clean (pi-deployment: Hardening check)
- [x] 5.2 `backup.sh`, then `deploy.sh`; `check.sh` 0 failed; same pin

## 6. Verify and docs

- [ ] 6.1 Integration test (`app/integration_test/run.sh`) passes on the iOS Simulator and the Android emulator
- [ ] 6.2 QA rerun through the real Pi of F1–F6 and own messages (iOS Simulator admin, two Android emulators for Bob), recorded in `docs/qa/` with screenshots; no unused invite captured
- [ ] 6.3 README / architecture doc where behaviour changed; `cargo test --workspace`, clippy, fmt, `buf lint`, `flutter analyze`, `flutter test`, pbgen + frb no diff; `openspec validate qa-fixes --strict`
