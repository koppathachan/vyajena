## 1. Pi: token rules and providers

- [ ] 1.1 `UpdatePushToken` limits (4096 bytes, no whitespace/control, APNs 64–200 hex); `auth::revoke_device` clears the token; tests `device-identity/push-token-registration`
- [ ] 1.2 `PushProvider::wake` returns a `WakeOutcome`; `messaging::notify` clears a token the provider calls invalid, only if unchanged; `RecordingPush` can answer `TokenInvalid`; tests `pi-runtime/push-errors`
- [ ] 1.3 `push.toml` + credentials loading (`--push`, modes, one `.p8`, key parsing); tests `pi-runtime/push-configuration`
- [ ] 1.4 FCM provider (service-account JWT → OAuth token cache, `messages:send`, fixed text, error table, backoff gate) against a fake OAuth + FCM server in `server/tests/`; tests `pi-runtime/push-provider-interface`, `pi-runtime/push-errors`, `message-delivery/content-free-wake-pings`
- [ ] 1.5 APNs provider (ES256 JWT cache, HTTP/2 alert, production → sandbox on `BadDeviceToken`, error table) against a fake APNs server; same spec tags
- [ ] 1.6 `egress-proxy` subcommand (CONNECT allowlist, 403/405, no private addresses); providers through it in a test; tests `pi-runtime/egress-proxy`
- [ ] 1.7 `main.rs` wires `--push` (FCM/APNs/logging per platform, start log); README server section

## 2. Bridge and app

- [ ] 2.1 `bridge/src/api/core.rs` `set_push_token`; frb regenerated; bridge test against a test Pi
- [ ] 2.2 Dart `PushService` + `vyajena/push` channel (register on connect, rotation, permission once after join, clear on deny, delete on `Removed`, clear notifications on open); widget tests `app-ui/push-notifications` on a fake channel
- [ ] 2.3 Android: Firebase Messaging (BoM), Gradle reads `~/.vyajena-push/google-services.json` into resources, release gate, `VyajenaMessagingService`, channel "Messages", small icon, manifest
- [ ] 2.4 iOS: `Runner.entitlements` (`aps-environment`), AppDelegate push channel (authorization, APNs token hex, clear, unregister)
- [ ] 2.5 Android debug build + iOS simulator build compile; `flutter analyze`, `flutter test`

## 3. Pi deployment

- [ ] 3.1 `deploy/pi/push.toml`, `vyajena-egress.service`; `vyajena.service` runs with `--push`; review unit unchanged
- [ ] 3.2 `deploy.sh` copies `~/.vyajena-push/` keys (mode checks, never printed), installs the egress unit; `--rollback` drops `--push` for an older binary
- [ ] 3.3 `check.sh`: egress sandbox and allowlist probe, `vyajena.service` still loopback-only, push file modes, listener 8445, WARN for a platform without credentials
- [ ] 3.4 `server/tests/deploy_files.rs` for the new deploy files; shellcheck clean

## 4. Docs and hand-offs

- [ ] 4.1 README, `docs/architecture-decisions.md`, `docs/release.md` (push section only)
- [ ] 4.2 Tell app-release + owner: Android permissions, iOS Push capability, Play Data safety / App Store privacy, `google-services.json` on the build Mac
- [ ] 4.3 `openspec validate push --strict`, `cargo test --workspace`, clippy, fmt, shellcheck

## 5. Rollout (owner steps first)

- [ ] 5.1 Owner: Firebase in `vyajena`, Android app, FCM service account key, APNs `.p8`, files in `~/.vyajena-push/`
- [ ] 5.2 `deploy/pi/backup.sh`, `deploy/pi/build.sh`, `deploy/pi/deploy.sh --no-build`; `check.sh` 0 failed; real pin unchanged
- [ ] 5.3 Owner: new Play internal + TestFlight builds
- [ ] 5.4 Real check: closed app on the owner's Android phone and iPhone wakes with only "New message"; Pi journal shows device id + platform only
- [ ] 5.5 `push-handover.md` in the tracker's notes folder
