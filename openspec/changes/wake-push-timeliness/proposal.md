## Why

The owner reports that **some real-message push notifications do not arrive, on
both Android and iPhone** — intermittent, "some messages", since ~the 0.8.0
rollout. Investigation (read-only; see side-session note
`missing-message-pushes-diagnosis-2026-11-28`) ruled out the push send path: on the
live Pi, every wake ping the server attempted was accepted (297 pings, 0 dropped, 0
`token_invalid`, 0 errors, both FCM and APNs configured). A missed notification is
therefore **not a failed send** — it is a push the server never attempted because of
two gaps in the *decision* to push. Both are server-only and platform-agnostic,
which is why they show on both platforms:

1. **A dead/backgrounded stream still counts as "connected" for up to 90 s (the
   zombie-stream window).** The server pushes an offline device, but delivers on the
   stream to a device it believes is connected. When a phone is backgrounded or its
   network drops, its TCP/HTTP2 connection can go half-open: the app stops sending
   its 25 s heartbeats, but the server only reaps the stream after
   `stream_idle_timeout` = **90 s** (wire-transport: Heartbeats). For up to 90 s a
   message is written to a dead stream and **no push fires**; the push comes only
   after the stream is finally reaped and re-pinged. The live logs show heavy churn
   (one device `reopens=116`), so this window is hit often.

2. **A real message's push is suppressed by an unrelated nudge (rate-limiter
   starvation).** The server sends at most one wake ping per device per 60 s, and
   that single budget is shared across *all* ping reasons — catch-up sweep,
   own-access admin events, stream-loss re-pings, and real messages. So a catch-up
   or own-access nudge sent in the same minute **eats the budget**, and a genuinely
   new user-visible message that arrives seconds later gets **no push** — even
   though the device is still offline (the earlier nudge demonstrably did not bring
   it online). With the churn in (1) generating frequent nudges, real messages get
   starved.

Neither gap is in the FCM/APNs code, which already retries, backs off and cleans
dead tokens correctly. A separate notification microservice would not help; the fix
is in the server's push-decision logic. (Android also has a separate, Android-only
cause — the 0.8.0 move to a data-only wake that the app must render locally — which
is out of scope here and tracked separately.)

## What Changes

- **Detect a dead `Connect` connection in ~30 s instead of ≤90 s (fix 1).** The
  server adds **HTTP/2 keepalive PINGs** on the `Connect` transport: it PINGs an
  otherwise-idle connection on an interval and closes a connection whose PING goes
  unanswered within a short timeout. A live client's HTTP/2 stack answers PINGs
  automatically (no app change); a frozen app or half-open socket does not, so the
  server closes the stream, treats the device as offline, and the existing
  stream-loss re-ping fires its wake push promptly. The 90 s heartbeat-idle reap
  stays as a backstop. New protocol constants
  `HTTP2_KEEPALIVE_INTERVAL` (20 s) and `HTTP2_KEEPALIVE_TIMEOUT` (10 s), both
  operator-tunable.

- **Never let a nudge starve a real message's push (fix 2).** The per-device wake
  rate limit becomes **reason-aware**. A wake ping for a **newly-queued notifiable
  message** (the write path, and the stream-loss re-ping that delivers a pending
  message) is allowed once at least `NEW_MESSAGE_PING_MIN_INTERVAL` = **10 s** have
  passed since the last *message* ping to that device, and is **not** suppressed by
  a catch-up or own-access nudge in between. A **nudge** (periodic catch-up sweep,
  own-access admin event) keeps the existing 60 s per-device cadence and does not
  reset or consume the message interval. The content-free wake payload and the
  on-device `collapse_key` are unchanged, so a short message floor cannot cause
  notification spam — repeated pings collapse to one notification on the device.

Both changes are **server-only**: no proto change, no client change, no app release.
They take effect on a server deploy alone. They refine the wire-transport
"Heartbeats" requirement and the message-delivery "Content-free wake pings"
requirement, so they go through OpenSpec and are owner-reviewed before merge.

## Open decisions resolved (owner, 2026-11-28)

- Dead-stream detection: **HTTP/2 keepalive** (not a shorter idle timeout), because
  it catches a truly half-open socket that sends nothing, needs no client/proto
  change, and keeps the 90 s idle reap as a backstop.
- Rate limiter: **protect real messages** — a new message gets a 10 s floor and is
  never blocked by a nudge; nudges keep the 60 s cadence. (Not: shorten the global
  limit for every reason; not: leave it unchanged.)

## Capabilities

### Modified Capabilities

- `wire-transport`: the server SHALL keep the `Connect` connection alive with HTTP/2
  keepalive PINGs and close an unanswered connection within
  ~`HTTP2_KEEPALIVE_INTERVAL + HTTP2_KEEPALIVE_TIMEOUT`, detecting a dead or
  half-open connection well before the 90 s heartbeat-idle reap.
- `message-delivery`: the per-device wake-ping rate limit is reason-aware — a
  newly-queued notifiable message is floored at 10 s and is never suppressed by a
  catch-up or own-access nudge; nudges keep the 60 s cadence.

## Impact

- **Proto:** none.
- **Client:** none (its HTTP/2 stack already answers PINGs; it already heartbeats
  every 25 s).
- **Server:**
  - `runtime/mod.rs` (`Server::builder()` at ~:247): set
    `.http2_keepalive_interval(Some(HTTP2_KEEPALIVE_INTERVAL))` and
    `.http2_keepalive_timeout(Some(HTTP2_KEEPALIVE_TIMEOUT))` (operator-tunable via
    config, defaulting to the protocol constants).
  - `runtime/config.rs`: two new tunable limits + their logging in the effective-
    configuration line.
  - `protocol/src/wire_transport.rs`: `HTTP2_KEEPALIVE_INTERVAL` (20 s),
    `HTTP2_KEEPALIVE_TIMEOUT` (10 s).
  - `protocol/src/message_delivery.rs`: `NEW_MESSAGE_PING_MIN_INTERVAL` (10 s).
  - `push/mod.rs` `PingLimiter`: store a per-device last-*message* ping time in
    addition to last-*any* ping time; `take` gains the ping's class (Message vs
    Nudge) and applies the right interval.
  - `messaging/mod.rs`: thread the ping class through `ping_offline` — the write
    path (`notify_in` from `stream.rs`) and the stream-loss re-ping use `Message`;
    the sweep catch-up and `notify_own_access` use `Nudge`.
- **Observability (folded in, cheap):** log every push *decision*, including a
  suppressed one, with its reason (`looked_connected` / `muted` / `silent` /
  `rate_limited` / `catch_up_exhausted`) so a future miss can be read straight from
  the logs. (A gated-out push logs nothing today.)
- **Tests:** server integration tests — (a) a stream whose client stops answering
  is reaped via keepalive well before 90 s and the pending message is re-pinged;
  (b) a nudge sent <60 s ago does not suppress a new message's ping once the 10 s
  message floor passes; (c) a nudge still obeys its own 60 s cadence.
- **Ops:** **server-only deploy; no app release.** Existing behaviour otherwise
  unchanged. Verify keepalive traverses the rathole relay in staging/local before
  the real deploy (expected: rathole forwards TCP transparently, so HTTP/2 PINGs
  pass end-to-end).

## Relationship to other changes

- Complements `notification-reliability` (sweep catch-up + stream-loss re-ping) and
  `smarter-notifications` (kind/`silent`/mute gating): those decide *whether* a
  device should be pushed; this change makes the server notice the device is offline
  *in time* and stops a real push being starved by a nudge. It modifies the same two
  requirements those changes last touched and is written as the full end state.
- Out of scope: the Android-only data-wake local-render reliability (OEM/Doze), and
  the underlying stream churn (relay/tunnel/network), each tracked separately.
