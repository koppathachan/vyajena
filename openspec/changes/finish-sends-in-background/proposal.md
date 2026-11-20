## Why

When the app goes to the background the core stops at once (flutter-app D9). A
message still showing ⏳ at that moment stays queued until the user opens the app
again, so "send, then switch away" can leave it undelivered for hours. The owner
asked (2026-11-20) for a bounded grace period: keep sending in the background, for
at most one minute, or until the message is done, whichever comes first, and the
minute starts when the connection succeeds.

## What Changes

- **Finish pending sends before stopping.** On going to the background the app asks
  the core whether any own message is still unsent. If none, it stops the core as
  before. If some, it keeps the core running until each is accepted or failed, or
  until one minute after the connection succeeded, then stops it.
- **Bounded wait for a connection.** If the core is not connected within one minute
  of going to the background, the app stops it anyway (the owner's rule counts the
  minute from the connection; this cap is the app's own, so a phone with no
  signal does not keep the radio busy).
- **Back to the app ends the grace** and leaves the core running.
- **iOS asks the system for time** (`beginBackgroundTask`) for the grace period;
  Android keeps the process as it is. iOS grants roughly 30 seconds, not a minute:
  the one-minute limit is the app's ceiling, the system may end it sooner and the
  app stops the core when told.
- **New core call** `has_unsent` (client, bridge, Dart `hasUnsent`): whether any own
  message is still sending. No `.proto` change.

## Capabilities

### Modified Capabilities

- `app-ui`: new requirement "Finish sends in the background".
- `client-runtime`: new requirement "Unsent messages query".

## Impact

- Code: `client/src/client.rs` (+ store query reuse), `bridge/src/api/core.rs`
  (generated bindings regenerated), `app/lib/core/` (`hasUnsent`),
  `app/lib/app/lifecycle.dart`, a small background-task channel
  (`app/lib/app/background_task.dart`, `app/ios/Runner/BackgroundTask.swift`).
- Tests: `client/tests/`, `app/test/app/background_send_test.dart`.
- Ships with an app build; nothing on the server.
- Not verified here: the iOS native part (needs a device or simulator run).
