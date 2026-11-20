## 1. Proto and shared rules

- [x] 1.1 No `.proto` change

## 2. Spec

- [x] 2.1 `client-runtime`: ADD "Unsent messages query"; `app-ui`: ADD "Finish sends in the background"; `openspec validate finish-sends-in-background --strict`

## 3. Core and bridge

- [x] 3.1 `Client::has_unsent`; test naming `client-runtime/unsent-messages-query`
- [x] 3.2 `Core.has_unsent` in `bridge/src/api/core.rs`; `cd app && flutter_rust_bridge_codegen generate`

## 4. App

- [x] 4.1 `CoreService.hasUnsent` (real and fake)
- [x] 4.2 `app/lib/app/background_task.dart` (channel `vyajena/background`, fake for tests) and `ios/Runner/BackgroundTask.swift` + AppDelegate wiring
- [x] 4.3 `lifecycle.dart`: the wait, with `kBackgroundSendLimit` and `kBackgroundConnectWait`
- [x] 4.4 Widget tests `app/test/app/background_send_test.dart` naming `app-ui/finish-sends-in-the-background`

## 5. Verify

- [x] 5.1 `cargo build/test/clippy/fmt` (no proto change, so no `buf lint`), `flutter analyze`, `flutter test`, `openspec validate --all --strict`
- [ ] 5.2 iOS: build runs (`flutter build ios --no-codesign` or a simulator run); owner: try on a device
