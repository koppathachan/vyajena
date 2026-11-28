# Design

## The crash

Four crash reports from the owner's iPhone (app 1.9.0 build 14, iOS 27.2.1),
`EXC_BAD_ACCESS (SIGBUS)`, subtype `FS pagein error: 1 Operation not permitted`,
all on the background tokio worker:

```
walIndexTryHdr → walIndexReadHdr → walTryBeginRead → sqlite3PagerSharedLock →
btreeBeginTrans → sqlite3VdbeExec → sqlite3_step → rusqlite Rows::advance →
client::store::device_state::Store::group →
client::engine::Engine::timed_checks → Engine::on_tick → Engine::run
```

`FS pagein error: Operation not permitted` on a memory-mapped file is iOS Data
Protection denying a page-in: the file's class is `NSFileProtectionComplete`
(`CoreDataDir.prepare` in `app/ios/Runner/AppDelegate.swift` sets it on the folder
and every file; SQLite's `-wal`/`-shm` inherit it), which iOS makes unreadable a
few seconds after the device locks.

## Why the core is running while locked

`CoreLifecycle` (app/lib/app/lifecycle.dart) stops the core when the app is
backgrounded — unless a message is still unsent, in which case it keeps the core
running for up to a minute to finish the send (change
`finish-sends-in-background`). A send that cannot complete yet (a DM whose peer has
no device in it — stuck "waiting") keeps `hasUnsent()` true for the whole minute.
The core's periodic `on_tick` fires `timed_checks`, which reads the group from the
database. If the user locks the phone during that minute, the `Complete`-class
database pages are evicted and that read faults — SIGBUS.

## Decision

Keep `NSFileProtectionComplete` (owner, 2026-11-29). Stop the core the moment the
device locks, and do not begin the background finish-sends wait while the device is
already locked. This is correct, not just a workaround: the keys are
`WhenUnlockedThisDeviceOnly`, so the core cannot send (or do any crypto) while the
device is locked — the grace period has nothing to finish there. Stopping the core
is exactly what the app already does when iOS ends the background time
(`BackgroundTask` `expired`); this adds one more reason to stop.

Alternative considered and rejected: relax the database to
`NSFileProtectionCompleteUntilFirstUserAuthentication` (the class Apple suggests
for background access). It is a one-line fix but makes the local message history
readable while the phone is locked (after first unlock), weaker at rest than today
and inconsistent with the `WhenUnlockedThisDeviceOnly` keys. The owner declined it.

## Mechanism

- iOS: `BackgroundTask.observeProtectedData()` observes
  `UIApplication.protectedDataWillBecomeUnavailableNotification` and calls the Dart
  method `locked` on the `vyajena/background` channel, then ends the background
  task. A new channel method `protectedDataAvailable` returns
  `UIApplication.shared.isProtectedDataAvailable`.
- Dart: `BackgroundTaskApi` gains `Stream<void> get locked` and
  `Future<bool> protectedDataAvailable()`. `MethodChannelBackgroundTask` wires them;
  on a platform without the native side (Android, tests) `protectedDataAvailable`
  is `true` and `locked` never fires (a `MissingPluginException` is swallowed, as
  `begin`/`end` already are).
- `CoreLifecycle._decide`: if a message is unsent but protected data is already
  unavailable, finish at once instead of waiting.
- `CoreLifecycle._wait`: also end the wait (stop the core, give the time back) if
  `locked` fires, under the same `pause` guard as the other end conditions.

## Timing / race

`protectedDataWillBecomeUnavailable` is a *warning*: iOS still allows reads for a
short grace (seconds) after it, before eviction. `core.stop()` tears the engine
down well within that grace, so no `timed_checks` read reaches an evicted page.
This is the same signal Apple documents for exactly this purpose.

## Tests

Dart widget tests (no device needed), on the fake background task:
- Locking during a background send stops the core at once and returns the time.
- Pausing while already locked stops the core at once, with no wait.
The iOS Swift wiring is covered by building the app (it has no unit test harness);
it mirrors the existing `begin`/`end`/`expired` pattern.
