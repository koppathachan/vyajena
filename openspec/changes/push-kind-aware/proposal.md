## Why

A real user reported a push notification that arrived with no new message. On
the deployed 0.5.0 server the content-free wake push (message-delivery:
Content-free wake pings) fires for **any** undelivered queued envelope, control
envelopes included. But an MLS control envelope — a `COMMIT`, a `WELCOME` or a
`PROPOSAL` — is not a message a person wrote; it is epoch machinery the core
applies silently. Several ordinary events queue a control envelope to an offline
member and nothing else: linking or reinstalling a device (a commit that adds
its leaf), admin-recovery re-keying, the server's inactivity re-adds, and a
closed-DM sync. Each one wakes the member's phone with a user-facing "you have
activity" alert that resolves to nothing visible — exactly the complaint.

The owner chose visible-only pushes, no silent pushes (2026-11-16), and the push
is content-free because the server only ever sees ciphertext. So the server
cannot turn a control envelope into a quieter push; the only correct fix is to
**not alert at all** for a control-only backlog. The server already distinguishes
envelope kinds everywhere else in the delivery log (it counts only `APPLICATION`
entries for queue caps and expiry), so the data and the pattern are already
there.

## What Changes

- **The visible wake push becomes kind-aware.** The server SHALL send the
  content-free wake push to an offline device only when that device has a pending
  `APPLICATION` entry. A device whose only undelivered entries are control
  envelopes (`COMMIT`, `WELCOME`, `PROPOSAL`) is **not** pushed.
- **Nothing else changes about control delivery.** Control envelopes still wake a
  device that has an **open** `Connect` stream (foreground epoch currency is
  unaffected), and a control-only backlog still syncs on the device's next natural
  reconnect — the existing catch-up and group-resync paths already keep a device
  from wedging behind its group's epoch.
- **The sweep catch-up ping is kind-aware too.** The periodic catch-up wake ping
  (notification-reliability: Sweep catch-up wake ping) only considers devices with
  a pending `APPLICATION` entry, so a control-only backlog neither fires a catch-up
  ping nor spends the device's catch-up budget.

This revises the content-free-wake-pings rule, so it goes through OpenSpec and is
owner-reviewed before merge.

## Open decisions resolved

- A control-only backlog fires no visible push and no catch-up ping; it syncs on
  reconnect (consistent with the owner's visible-only-push decision, 2026-11-16).

## Proto files touched

- None. `Envelope.Kind` (`proto/vyajena/v1/messaging.proto`) already carries the
  distinction; no wire or client change is needed.

## Capabilities

### Modified Capabilities

- `message-delivery`: the content-free wake push is sent only for an `APPLICATION`
  backlog; a control-only backlog fires no push.

## Impact

- **Proto:** none.
- **Server:** `messaging::log::backlogged_devices` filters to devices with an
  `APPLICATION` delivery (sweep catch-up); a new `messaging::log`
  `has_application_backlog` gate in the offline branch of `messaging::notify_in`,
  covering both the write-path push and the sweep push. The open-stream wake
  (`hub.wake`) and the queued entries are untouched. New test
  `message-delivery/content-free-wake-pings` proving a control-only backlog for an
  offline device produces no push while an application backlog does.
- **Core / bridge / app:** none.
- **Ops:** server-only; deploy the server, no app change. Existing data unchanged.
