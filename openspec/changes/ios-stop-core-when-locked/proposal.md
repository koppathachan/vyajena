## Why

On iPhone the app crashes with `EXC_BAD_ACCESS / SIGBUS — "FS pagein error:
Operation not permitted"` while sending. All four crash reports from the owner's
device (app 1.9.0 build 14, iOS 27.2.1) have the same stack: the background tokio
worker in `sqlite3_step → … → walIndexTryHdr` under
`client::engine::Engine::timed_checks → on_tick`, i.e. the core reading its SQLite
database (WAL, memory-mapped `-shm`).

The core's database directory is protected with `NSFileProtectionComplete`
(app-ui: Data on the phone). iOS makes `Complete`-class files unreadable a few
seconds after the device **locks**. Separately, the finish-sends-in-background
change keeps the core running for up to a minute after the app is backgrounded
when a message is still unsent. A message that cannot be sent yet — e.g. a DM whose
peer has no device in it, so it is stuck "waiting" — keeps that whole minute open.
If the user locks the phone during it, iOS evicts the `Complete` database pages and
the core's next background database read page-faults: SIGBUS. It is iPhone-only
(Android has no such lock-time eviction) and not caused by any one recent PR — it
is a latent incompatibility between the background grace period and
`NSFileProtectionComplete`, now hit because a stuck "waiting" send holds the grace
open long enough to overlap a lock.

The identity, auth and MLS keys are already `WhenUnlockedThisDeviceOnly`
(client-runtime: Key storage), so the core cannot do any crypto while the device
is locked anyway: running it there is useless as well as crash-prone.

The owner chose (2026-11-29) to **keep `NSFileProtectionComplete`** (no weakening
of at-rest protection) and instead stop the core whenever the device locks.

## What Changes

- **Stop the core when the device locks.** While the app is keeping the core
  running in the background to finish a send, if iOS reports that its protected
  data became unavailable (the device locked), the app stops the core at once and
  gives the background time back — exactly as it already does when the system ends
  the background time. It does not start that background wait at all if protected
  data is already unavailable.
- **iOS protected-data signal.** The iOS side observes
  `UIApplication.protectedDataWillBecomeUnavailableNotification` and forwards it to
  the Dart lifecycle over the existing `vyajena/background` method channel, and
  answers whether protected data is available now
  (`UIApplication.isProtectedDataAvailable`). Android and widget tests, which have
  no such state, report "available" and never lock.
- **No change to the protection class.** The database stays
  `NSFileProtectionComplete`. No core, bridge or `.proto` change.

## Open decisions resolved

- Keep `NSFileProtectionComplete` and stop the core on lock, rather than relax the
  database to `NSFileProtectionCompleteUntilFirstUserAuthentication` (owner,
  2026-11-29).

## Capabilities

### Modified Capabilities

- **app-ui**: a new requirement, "Stop the core when the phone locks".
