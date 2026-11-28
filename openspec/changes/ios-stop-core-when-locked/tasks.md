# Tasks

## 1. iOS protected-data signal
- [x] 1.1 In `app/ios/Runner/AppDelegate.swift`, add a `protectedDataAvailable`
  method to the `vyajena/background` channel handler returning
  `UIApplication.shared.isProtectedDataAvailable`.
- [x] 1.2 Add `BackgroundTask.observeProtectedData()` that observes
  `UIApplication.protectedDataWillBecomeUnavailableNotification`, invokes the Dart
  method `locked` on the channel and ends the background task; call it where the
  channel is created in `didInitializeImplicitFlutterEngine`.

## 2. Dart background-task API
- [x] 2.1 In `app/lib/app/background_task.dart`, add `Stream<void> get locked` and
  `Future<bool> protectedDataAvailable()` to `BackgroundTaskApi`, and wire them in
  `MethodChannelBackgroundTask` (handle the incoming `locked` call; invoke
  `protectedDataAvailable`, defaulting to `true` on `MissingPluginException`).

## 3. Lifecycle
- [x] 3.1 In `app/lib/app/lifecycle.dart` `_decide`, when a message is unsent but
  `protectedDataAvailable()` is false, finish at once instead of waiting.
- [x] 3.2 In `_wait`, subscribe to `background.locked` and finish (stop the core,
  end the background time) under the same `pause` guard as the other end
  conditions.

## 4. Tests
- [x] 4.1 Extend `app/test/support/fake_background_task.dart` with `lock()`,
  `locked`, and a settable `available` backing `protectedDataAvailable()`.
- [x] 4.2 In `app/test/app/background_send_test.dart`, add: locking during a
  background send stops the core at once (`// spec: app-ui/stop-the-core-when-the-phone-locks`);
  pausing while already locked stops at once with no wait.
- [x] 4.3 `cd app && flutter analyze` clean and `flutter test` green.
- [x] 4.4 Build the iOS app on a simulator to confirm the Swift wiring compiles.

## 5. Validate
- [x] 5.1 `openspec validate ios-stop-core-when-locked --strict` passes.
