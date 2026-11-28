# Design — wake-push-timeliness

## Context

Two server-only gaps drop real-message wake pushes without any send failure. Both
are in the decision to push, both are platform-agnostic (hence "both Android and
iPhone"). Evidence: side-session note `missing-message-pushes-diagnosis-2026-11-28`
(live-Pi logs: 297 pings, all `sent`, 0 failures; `reopens=116` on one device).

## Fix 1 — dead-stream detection via HTTP/2 keepalive

### Problem
`notify_in` (`server/src/messaging/mod.rs:85`) wakes open streams via
`state.hub.wake(&devices)` and only pushes the **offline** remainder. "Offline" =
"no open `Connect` stream in the hub". The hub's view lags reality: a backgrounded
phone or a dropped network leaves a half-open HTTP/2 connection. The client stops
its 25 s app-level heartbeats, but the server reaps the stream only after
`stream_idle_timeout` = 90 s (`stream.rs:300` `idle()`, wire-transport: Heartbeats).
For that ≤90 s the device looks connected, the message is written to a dead stream,
and **no push fires**. The stream-loss re-ping (`stream.rs:558-569`) fires the push
only after the reap.

### Decision (owner): HTTP/2 keepalive, not a shorter idle timeout
Set HTTP/2 keepalive on the `Connect` transport so the server actively PINGs an idle
connection and closes it if the PING is unanswered within a short timeout:

```
Server::builder()
    .http2_keepalive_interval(Some(HTTP2_KEEPALIVE_INTERVAL)) // 20 s
    .http2_keepalive_timeout(Some(HTTP2_KEEPALIVE_TIMEOUT))   // 10 s
    ...
```

Dead-connection detection becomes ~`interval + timeout` ≈ **30 s** (worst case), vs
90 s today. On close, the existing stream-loss re-ping pushes the pending message.

Why keepalive over lowering `stream_idle_timeout`:
- **Catches true half-open sockets.** A frozen app's socket sends nothing; a shorter
  app-level idle timeout would still wait for the 25 s heartbeat to lapse and relies
  on the client's clock. A transport PING with an unanswered timeout detects a dead
  peer directly.
- **No client or proto change.** HTTP/2 PING/PONG is handled by the client's `h2`
  stack automatically; app code is not involved. A live-but-quiet client answers and
  is not reaped.
- **Keeps the 90 s idle reap as a backstop**, unchanged — belt and braces.

Rejected: lowering `stream_idle_timeout` 90→30 s (alone) — more aggressive reaping
of live-but-quiet streams, more reconnect churn, and still blind to a socket that is
half-open but not yet idle by the app-heartbeat clock.

### Relay note
The `Connect` connection runs through the GCP rathole relay, which forwards raw TCP.
HTTP/2 PING frames are end-to-end between server and client and pass through the
tunnel transparently; a dead client leg does not answer even if the server↔relay TCP
stays up. **Verify in staging/local** (task) that keepalive closes a killed client
within ~30 s through the relay before the real deploy.

### Tunability
`HTTP2_KEEPALIVE_INTERVAL` and `HTTP2_KEEPALIVE_TIMEOUT` are protocol constants, made
operator-tunable in `server.toml` like `stream_idle_timeout` and friends, and logged
in the effective-configuration line at startup.

## Fix 2 — reason-aware rate limiter (no nudge starves a message)

### Problem
`PingLimiter::take(device_id, now)` (`server/src/push/mod.rs:228`) enforces one ping
per device per `WAKE_PING_MIN_INTERVAL` = 60 s across **all** reasons. A catch-up
sweep nudge, an own-access nudge, or a stream-loss re-ping consumes the single
budget, so a real message queued seconds later is dropped — although the device is
still offline, i.e. the prior nudge did not wake it.

### Decision (owner): protect real messages with a 10 s floor; nudges keep 60 s
Make the limiter reason-aware. Two classes of ping:
- **`Message`** — a freshly-queued notifiable (`APPLICATION`, not `silent`, not
  muted) message. Callers: the write path (`notify_in` from `stream.rs:265`) and the
  stream-loss re-ping (`stream.rs:569`, which exists to deliver a pending message).
- **`Nudge`** — a periodic catch-up sweep ping (`sweep.rs:138`) and an own-access
  admin-event ping (`notify_own_access`).

Limiter state per device: `last_message_at` and `last_any_at`.
- A `Message` ping is allowed iff `now − last_message_at ≥ NEW_MESSAGE_PING_MIN_INTERVAL`
  (10 s). Nudges in between are ignored for this decision. On allow, set both
  `last_message_at` and `last_any_at`.
- A `Nudge` ping is allowed iff `now − last_any_at ≥ WAKE_PING_MIN_INTERVAL` (60 s).
  On allow, set `last_any_at` only (a nudge does not reset the message floor).

Result: a real message is rate-limited only by a *prior real message* (≤10 s), never
by a nudge. Nudges stay at 60 s. The content-free payload uses `collapse_key="wake"`,
so bursts collapse to one on-device notification — the 10 s floor caps API calls, not
user-visible alerts.

Rejected: shorten the single global interval 60→10 s for every reason — simplest but
makes nudges 6× noisier for no benefit; and leaving it unchanged — a nudge would keep
starving real messages even after fix 1.

### Threading the class
Add a `PingClass { Message, Nudge }` carried alongside `Why` into `ping_offline`.
`notify`/`notify_in` gain the class (default `Message` for the write path); the sweep
passes `Nudge`; `notify_own_access` passes `Nudge`; the stream-loss re-ping passes
`Message`. `PingLimiter::take` takes the class and applies the matching interval.

## Observability (folded in)
Today a suppressed/gated push logs nothing, so a miss is invisible in the logs. Add a
single structured `debug`/`info` line at each skip in `ping_offline` with the reason:
`looked_connected` (handled before `ping_offline`, log at the `hub.wake` split),
`muted`, `silent`/no-notifiable-backlog, `rate_limited` (with class), and a per-
outcome counter alongside the existing `sent`/`dropped`/`token_invalid`. No behaviour
change; makes fixes measurable and the next report correlatable.

## Risks
- Keepalive too aggressive → reaping live clients on lossy mobile networks. Mitigated
  by a 10 s unanswered-PING timeout (a live client answers in one RTT) and tunable
  constants; the client reconnects and resumes (session resume) if wrongly reaped.
- 10 s message floor → at most one extra push per 10 s per device in a burst;
  collapsed on device, negligible API cost on a single-Pi deployment.
- Both are server-only and config-tunable, so a bad value is a config roll, not a
  code roll.

## Test plan
Server integration tests (no device), against a real local server with a fake push
provider recording pings:
1. **Keepalive reap:** open a `Connect` stream, stop answering at the transport
   (drop the client task without a clean close), assert the server closes the stream
   and the pending message is re-pinged in ≪90 s. (If transport-level simulation is
   impractical in-test, assert the builder is configured with the keepalive options
   and cover the re-ping-on-close path directly.)
2. **Nudge does not starve a message:** send a `Nudge` ping, then <60 s later queue a
   real message after the 10 s message floor → a `Message` ping fires.
3. **Message floor:** two messages <10 s apart → one ping; ≥10 s apart → two.
4. **Nudge cadence unchanged:** two nudges <60 s apart → one ping.
Follow repo convention: tests in `server/tests/`, tagged
`// spec: message-delivery/content-free-wake-pings` and
`// spec: wire-transport/heartbeats`.

## Deployment
Server-only. Build with `deploy/server/build.sh`, deploy per the owner's process;
no app release. Behaviour-compatible with existing clients and data.
