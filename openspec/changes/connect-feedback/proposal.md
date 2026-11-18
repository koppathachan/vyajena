## Why

The app bar's connection indicator has three states, but two of them look
almost the same. `connecting` is `cloud_sync_outlined` and `offline` is
`cloud_off_outlined`, both drawn static in the same muted `onSurfaceVariant`
grey. At a glance, on a 20 px app-bar glyph, "connecting" reads as "broken":
a user waiting out a reconnect backoff cannot tell the app is working from
the app having given up.

This is a UI-only gap. The core state is already right: during backoff the
engine stays in `Connecting` (it sets the next-attempt time and does not emit
`Disconnected` while it waits), and `CoreService` already exposes
`connection`, `removed` and `reconnectNow()` (which skips the backoff wait).
Nothing in the protocol, the Pi, the bridge or the Rust core changes.

## What Changes

- **Connecting looks like working, not broken (app):** the connecting
  indicator animates (a gentle pulse) and uses an active colour, distinct
  from the muted static colour of offline. Connected stays a static
  "done" glyph; offline stays a static muted glyph; removed is unchanged.
- **Tap to retry (app):** tapping the indicator while connecting or offline
  (and not removed) calls `reconnectNow()`, which skips the reconnect
  backoff wait. This is the app's first manual-refresh affordance. When the
  indicator is connected, or this device was removed, tapping does nothing.
- **Pull to refresh (app):** pulling the chat list down calls the same
  `reconnectNow()`, so the main screen has a familiar manual-refresh gesture.
- **Removed stays as-is:** the "removed from the group" look, label and the
  fact that the core has stopped connecting are unchanged.
- **Out of scope:** any core, bridge, Pi or protocol change; a separate
  "retrying in N s" countdown (the core does not expose the next-attempt
  time to Dart, and adding it is a later change if wanted).
- Details and decisions: `design.md`.

## Open decisions resolved

- Connecting's colour: the active brand colour (`primary`), with motion
  carrying "in progress", because the app's neutral slate scheme has no
  strong second hue to spend on a third state (design D1).
- Manual refresh affordance: both tap-the-indicator and pull-to-refresh on
  the chat list, both calling `reconnectNow()` (design D2).

## Proto files touched

None. This change touches `app/lib/` and `app/test/` only.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `app-ui`: the connection indicator is visually distinct in each state and
  offers tap-to-retry and pull-to-refresh manual reconnects.

## Impact

- `app/lib/app/connection_indicator.dart`: animate connecting, give it an
  active colour, make it tap-to-retry.
- `app/lib/chat/chat_list_screen.dart`: pull-to-refresh on the chat list.
- `app/test/app/connection_indicator_test.dart` (new): the three visual
  states, the retry tap, and the no-op cases.
- No `.proto`, `pb.rs`, bridge, frb or Rust change.
