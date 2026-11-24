## Context

The wake-push path on `main` today (verified in the 2026-11-23 diagnosis):

- A wake push is requested **once**, at message-store time, for a device that has
  no open `Connect` stream and has a push token (message-delivery: Content-free
  wake pings). One push per device per 60 s; extras in that window are dropped;
  pings collapse into one on the device.
- "No open stream" is decided by stream reaping: a stream is closed after 90 s
  with no client frame (`STREAM_IDLE_TIMEOUT`), heartbeat every 25 s.
- The 60 s sweep (`sweep.rs`) handles expiry/inactivity; it does **not** re-ping
  offline devices that still have undelivered entries.

Two silent gaps follow:

1. **Zombie window.** An app backgrounds; its stream is not yet reaped (up to
   ~90 s). Messages queued in that window see an "open stream", so no push is
   requested. When the stream is reaped, nothing re-examines the backlog, so those
   messages never get a push.
2. **Lost/collapsed single ping.** The one push can be dropped by the 60 s limit
   (a burst), collapsed with an earlier one, or simply lost by FCM/APNs. There is
   no retry, so the message waits until a manual reopen.

The app already catches up correctly once connected (message-delivery: Ordered
delivery and catch-up). The missing behaviour is making the device reconnect:
(a) the server re-pinging when it realises a device with a backlog is gone, and
(b) a sweep that keeps nudging an offline device with a backlog.

## Decisions

### D1. Re-ping on stream loss when a backlog exists

When the server closes or reaps a device's `Connect` stream — idle-timeout reap,
or any disconnect — it SHALL check whether that device still has undelivered
queued entries, and if so request a wake push for it, subject to the existing
one-per-device-per-60 s limit. This directly closes the zombie window: the burst
that arrived while the stream was quietly dead is pinged the moment the server
accepts the device is offline, instead of never.

- It reuses the existing single content-free push; no new payload, no content.
- The 60 s limit still applies, so a reap right after a store-time push does not
  double-send; the catch-up simply covers the case the store-time push missed.

### D2. A sweep-based catch-up ping

The periodic sweep SHALL, for each device that has no open stream and has
undelivered queued entries and a push token, request a wake push, rate-limited by
the same one-per-device-per-60 s rule, and SHALL repeat this on subsequent sweeps
until either the device reconnects and acks its backlog (the queue drains) or a
bounded number of catch-up attempts for that backlog is reached (`CATCH_UP_MAX`,
e.g. a handful, to avoid pinging a device whose app is uninstalled forever —
platform "unregistered" errors already clear the token, push: Push errors).

- This turns the single store-time push into "push, and keep nudging a few times
  if it doesn't land", which is what "nothing dropped silently" requires.
- It is cheap: the sweep already runs every 60 s and already scans per-device
  queues for expiry; this adds "has backlog && offline && under the attempt bound
  → ping (rate-limited)".
- The attempt counter is per device per backlog-run and resets once the device
  drains, so a later message starts a fresh catch-up budget.

### D3. Keep the 90 s idle timeout and the 60 s limit (optional tuning only)

The diagnosis noted two optional tunings: shortening `STREAM_IDLE_TIMEOUT` (so the
zombie window is smaller) and making the limit/collapse per-conversation (so two
different chats don't suppress each other's single ping). Neither is required once
D1 + D2 land — the catch-up covers the window and the retries cover a suppressed
ping — and both have costs (more reconnect churn; more pushes). They are left as
future tuning, out of scope here, so this change is purely additive scheduling.

### D4. Client reconnects promptly (already implemented)

The server cannot make an offline device reconnect except through the wake push;
the device must also reconnect on its own when it can. The app SHALL re-open
`Connect` on returning to the foreground, on regaining network, and after its
stream drops, with a bounded backoff, so that a delivered wake push — or simply
coming back online without one — drains the backlog quickly rather than on the
next manual action.

This behaviour is **already present in the core and app** and is not re-built by
this change; the change only adds a test pinning it:

- The engine loop reconnects whenever the link is gone, after a drop
  (`drop_link`) and with an exponential, jittered, bounded backoff reset on a
  successful connect (`client/src/net/backoff.rs`: 250 ms → 30 s), so a dropped
  stream reconnects on its own (client-runtime: Connection lifecycle).
- `Client::reconnect_now` / the bridge `reconnect_now` let the app skip the
  backoff wait when the network returns.
- The app lifecycle (`app/lib/app/lifecycle.dart`, flutter-app design D9 +
  change `finish-sends-in-background`) stops the core on pause (after finishing
  any unsent message) and **restarts it on resume**, and calls `reconnectNow` on a
  network-up change (qa-fixes design D6); a tapped wake push opens the app, which
  starts the core here. So resume, network regain and a delivered push all drain
  the backlog without a manual action.

### D5. The catch-up attempt counter is durable

The per-device catch-up attempt count of D2 is kept in SQLite (a small `catchup`
table: device id → attempts), not only in memory. A server restart therefore does
not forget a live device's progress nor hand a device whose app is gone a fresh
`CATCH_UP_MAX` budget on every restart. The counter is bumped on each sweep
catch-up ping, and reset — the row deleted — when the device's backlog drains
(no `deliveries` rows left) or the device is currently connected (it is receiving
on its stream). The reset is done in the sweep, in the same transaction, so it
costs no extra work on the hot `Ack` path:

```sql
-- once per sweep, before choosing who to catch-up-ping:
DELETE FROM catchup
 WHERE device_id NOT IN (SELECT device_id FROM deliveries)   -- drained/expired
    OR device_id IN (<connected device ids>);                -- receiving now
```

The wake-push rate limiter (one per device per 60 s) stays in memory in the
`Hub`; that is fine to reset on restart (it only governs how *soon* a push may be
re-sent). Only the attempt *bound* needs durability, because it is what stops an
unbounded series of pings to a device that never comes back.

### D6. Why not a durable queue library (`kyu`)

The owner asked whether to re-architect this around a durable server-side delivery
queue, specifically the `kyu` library (github.com/koppathachan/kyu). Evaluated on
2026-11-24 and declined for this change:

- The undelivered-message queue is **already durable**: the SQLite `envelopes` /
  `deliveries` / `cursors` tables (message-delivery: Per-device queues; Accepted
  means durable). It is ack-drained and expires entries after `QUEUE_ENTRY_TTL`, so
  it is a pending-delivery queue, never a message archive — exactly the invariant
  (config.yaml: "the server … never history"). The reliability gap is wake-push
  *scheduling* (the zombie window and the un-retried single ping), not durability.
- `kyu` would not improve durability here and would cost a great deal: it is a Go
  service exposing gRPC contracts, not a Rust crate ("neither contract is a
  library: both are gRPC"), so it would run as a separate process on the Pi; its
  engine keeps no durable state and needs a storage backend the deployment supplies
  (only an in-memory reference ships), so durability would still be ours to write;
  and its dead-letter-queue model *retains* exhausted messages, which for
  undelivered ciphertext would be the message archive the invariant forbids. Its
  single-message-lease + DLQ model also does not fit per-device fan-out + a
  monotonic `server_seq` + cursor/ack catch-up.

So this change keeps the existing SQLite store and adds only scheduling (D1, D2)
and a durable attempt counter (D5); no new dependency, no new process. The full
assessment was reported to the owner before implementation and the no-kyu decision
confirmed.

```mermaid
sequenceDiagram
    participant A as App (backgrounded)
    participant S as Server
    Note over A,S: stream still "open" (zombie window)
    S->>S: queue M1, M2 for A's device (stream open -> no push)
    S->>S: 90s idle: reap A's stream; backlog present -> request wake push (D1)
    S-->>A: wake push "New message"
    Note over S: if push lost/collapsed, next sweep re-pings (D2)
    A->>S: on push / resume / net regain: re-open Connect (D4)
    S-->>A: catch-up delivers M1, M2 in order; A acks; queue drains
```

## Risks / Trade-offs

- **[A device whose app is uninstalled keeps a backlog]** → `CATCH_UP_MAX` bounds
  the catch-up pings; platform `UNREGISTERED`/410 responses already clear the
  token (push: Push errors), after which no push is attempted at all.
- **[Extra pushes vs the old single-shot]** → still one-per-60 s per device, and
  only while a real undelivered backlog exists; a connected device is never
  pinged. Net traffic rises only for devices that genuinely missed messages.
- **[Interaction with mute]** → a catch-up push is still gated by the mute check
  (mute-notifications: A muted conversation gets no wake push); muting keeps a
  conversation silent on the catch-up path too.
- **[Reconnect churn on the client]** → bounded backoff on the reconnect policy
  avoids a tight loop when the network is flapping.

## Migration Plan

Add the re-ping-on-reap and the sweep catch-up (with `CATCH_UP_MAX`) on the
server, backed by one additive SQLite migration (`0009_catchup.sql`, the durable
counter of D5). No proto regeneration and no core/app change (the reconnect policy
of D4 already exists). The migration only creates a new, initially-empty table, so
it is a safe forward-only upgrade with no data move and no `--reset`; an older
server simply has no `catchup` rows until it runs. Deployable independently of the
app (older apps still catch up on reconnect; the extra pushes just reach them
sooner). Builds serialize on the shared external-disk cargo target.
