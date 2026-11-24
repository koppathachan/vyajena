## Why

Users report "sometimes I don't get a notification". A diagnosis on `main`
(2026-11-23) found a real silent-gap window, not flaky push:

- The wake push is requested **only when a message is first stored** for an
  offline device, and is rate-limited to one per device per 60 s (extras are
  dropped), and pings to a device collapse into one.
- But a device is only treated as offline after its `Connect` stream is reaped:
  the server closes a stream that has received nothing for 90 s
  (`STREAM_IDLE_TIMEOUT`), with a 25 s heartbeat. So for up to ~90 s after an app
  is backgrounded, its stream is still "open" and messages queued in that window
  get **no** wake push at all.
- When the stream is finally reaped, the server does **not** go back and ping the
  device for the backlog that arrived during the zombie window. The 60 s sweep
  (`server/src/messaging/sweep.rs`) does not re-ping offline devices that have
  undelivered entries either.

Result: a message that lands in the zombie window, or whose single ping was
dropped by the rate limit or lost by the platform, can sit in the queue with no
push until the user next opens the app by hand — "silent until manual reconnect".
The app otherwise catches up correctly once it connects (message-delivery:
Ordered delivery and catch-up); the gap is purely *that the device is never told
to reconnect*.

## What Changes

- **Re-ping when a stream is lost with a backlog (server).** When a device's
  `Connect` stream closes or is reaped for idle timeout while that device still
  has undelivered queued entries, the server SHALL send a catch-up wake push
  (subject to the existing one-per-device-per-60 s limit). This closes the
  zombie-window gap: the messages that arrived while the stream was quietly dead
  now trigger a push as soon as the server notices the device is gone.
- **A sweep catch-up ping (server).** The periodic sweep SHALL, for each device
  with no open stream and undelivered backlog, send a catch-up wake push, rate-
  limited per device, retried on each sweep until the device drains its backlog
  (reconnects and acks) or a bounded number of catch-up attempts is reached. A
  ping dropped by the 60 s limit, collapsed away, or lost in transit is therefore
  retried rather than lost once.
- **Prompt reconnection (client).** On returning to the foreground, on regaining
  network, and after its stream drops, the app SHALL promptly re-open `Connect`
  and catch up, rather than waiting for the next user action — so a delivered
  wake push, or simply coming back online, reliably drains the backlog.
- **No message is dropped silently.** The invariant stated here: a stored,
  undelivered entry for an offline device with a push token SHALL keep producing
  catch-up pings (within the rate limit and attempt bound) until it is delivered
  or expires, so nothing sits pinged-once-and-forgotten.

## Open decisions resolved

- Fix shape: re-ping on stream loss + sweep-based catch-up, keeping the one-per-60 s
  rate limit and collapse (owner diagnosis, 2026-11-23). Shortening the 90 s idle
  timeout and making the limit/collapse per-conversation are noted as optional
  tuning (design D3), not required here.

## Proto files touched

- **None.** This is server scheduling (when a wake push is (re)sent) and client
  reconnection behaviour; it adds no wire fields and contends for no field numbers
  with the in-flight changes (`remove-closed-dm`, `mute-notifications`,
  `replace-user`, `self-hostable`, `video-messages`). The push payload is
  unchanged — still the one content-free "New message".

## Capabilities

### Modified Capabilities

- `message-delivery`: adds the re-ping-on-stream-loss rule, the sweep catch-up
  ping, and the no-silent-drop invariant, on top of Content-free wake pings.
- `client-runtime`: adds prompt reconnection after foreground / network regain /
  stream drop.

## Impact

- **Server:** `server/src/messaging/mod.rs` and `server/src/messaging/sweep.rs`
  (re-ping on stream close/reap when a backlog exists; per-device catch-up ping in
  the sweep with a per-device attempt bound and the shared 60 s limit);
  `server/tests/` (a device with a backlog and no stream gets a catch-up ping;
  the ping stops once the backlog drains or the bound is hit; connected devices
  are never pinged). Interacts with `mute-notifications`: a catch-up push is still
  subject to the mute check (a muted conversation is never pinged, catch-up
  included).
- **Core/app:** `client/src/net/conn.rs` reconnection policy; the app's lifecycle
  (`app/lib/app/lifecycle.dart`) re-opens the stream on resume and on
  connectivity change. No proto regeneration.
- **Ops:** server-side; no data migration, no new credentials. Works with or
  without a real push provider (a logging provider simply logs the catch-up ping).
