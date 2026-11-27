## Context

`push-kind-aware` made the wake push depend on the envelope **kind**: an offline
device is pushed only when it has a pending `APPLICATION` entry, so MLS control
envelopes (`COMMIT`/`WELCOME`/`PROPOSAL`) do not alert. The residual bug is that
"`APPLICATION`" is a wire kind, not "a message a person wrote". Three `Content`
bodies ride in an `APPLICATION` envelope but produce nothing for the user to see:
`Profile`, `GroupSettings` and `Delete`. The server, seeing only ciphertext, cannot
tell them from a text/media message. So the fix needs a plaintext discriminator the
sender sets and the server reads — exactly how `Envelope.deletes` already works for
message deletion.

## Decisions

### D1 — A sender-set plaintext `silent` boolean, default false

Add `bool silent = 12;` to `Envelope`, in the client-set section. The sending client
sets `silent = true` iff the envelope is `APPLICATION` and its `Content` body is a
`Profile`, a `GroupSettings` or a `Delete`. Every message body (`Text`, `Image`,
`Audio`, `Video`) and every control envelope leaves it false.

Rationale for the polarity (silent, default false — not "notifiable", default true):
protobuf bools default to false, and the default must be the **safe** value. With
`silent` default false, an envelope from an old client (which never sets the field)
is treated as notifiable — messages still notify. If the field were `notifiable`
(default false), an old client would mark every message non-notifiable and silence
all pushes — a regression. So `silent` it is: the fix is opt-in per envelope and
takes effect as soon as the *sender* is updated; an old server ignores the field and
behaves as today (`buf breaking` clean, additive).

The server trusts `silent` only to **withhold** a push — the conservative direction.
A malicious or buggy sender could at worst mark its own real message `silent` and so
not wake its recipients (it only hurts that sender's deliverability, like sending no
message at all), or mark a silent body non-silent and cause a stray push (today's
behavior). It can never use `silent` to make the server *drop* or *forge* an entry;
the envelope is still queued and delivered in `server_seq` order regardless.

### D2 — The gate reads a notifiable backlog, in the same chokepoint

`notify_in`'s offline branch already asks `log::has_application_backlog(device)`
(push-kind-aware D2). Narrow that query to **notifiable** entries by adding
`AND e.silent = 0` to the `kind = APPLICATION` join. One gate, one chokepoint
(write path, key rotation, stream re-ping, sweep all funnel through
`notify`/`notify_in`); no caller needs to know the body type. The open-stream wake
(`hub.wake`) still runs before the filter, so a foreground device still receives
every kind on its stream — a live profile/settings/delete still applies instantly
when the device is connected.

Because the check reads the device's whole notifiable backlog (not just the
triggering envelope), a device that already has a real message waiting is still
pushed when a later silent envelope arrives — correct, there is a message to see.

### D3 — The sweep filters by `silent` too

`log::backlogged_devices` (notification-reliability: Sweep catch-up wake ping) gets
the same `AND e.silent = 0`. Otherwise the sweep would keep nudging — and burning the
durable catch-up budget on — a device whose only backlog is silent, and
`prune_catchup` would never reset it (it still "has a backlog"). With the filter, a
silent-only backlog is invisible to the sweep, exactly as a control-only backlog is.

### D4 — Persist `silent` on the envelope row; leave cap and TTL alone

The gate queries a device's backlog long after the envelope was accepted (the sweep,
and the cross-group `has_application_backlog`), so `silent` must be stored per
envelope, not recomputed. Migration `0013_silent_envelopes.sql`:
`ALTER TABLE envelopes ADD COLUMN silent INTEGER NOT NULL DEFAULT 0 CHECK (silent IN
(0, 1));`. Existing rows default to 0 (notifiable) — the current behavior, and
control rows are excluded by `kind` anyway. `log::insert_envelope` writes it from the
accepted envelope's `silent` field (the server-generated external-Remove proposal
path passes false; it is a `PROPOSAL`, excluded by kind regardless).

`application_queue_len` (the per-device `QUEUE_CAP`) and `expire` (the application
TTL) are deliberately **not** filtered by `silent`: a silent application envelope is
still real queue load and still expires like any application entry. Only the push
decision changes.

### D5 — Client set-site

The client builds exactly one `APPLICATION` envelope, in `engine::outbox`, already
setting `deletes: messaging::delete_target(&message.content)`. Add
`silent: messaging::is_silent_content(&message.content)` beside it, where
`is_silent_content` decodes the content and returns true for
`Decoded::Profile | Decoded::GroupSettings { .. } | Decoded::Delete`. This mirrors
`delete_target` (also a content-inspecting plaintext hint) and keeps the body→wire
mapping in `messaging`, next to the receive-side `Decoded::… => None` that already
marks the same three bodies as non-messages.

## Risks / trade-offs

- **Old sender, new server:** an un-updated client still sends profile/settings/
  delete with `silent` unset, so those still phantom-push until the app is updated.
  Acceptable and self-healing; the MVP has few users and a coordinated app release.
- **A long-offline device whose only activity is a silent envelope** is not woken and
  sees the new profile/name/tombstone only on its next natural reconnect — the same,
  accepted trade-off push-kind-aware already makes for control envelopes.

## Migration / rollout

Additive proto field + additive column (default 0): no data migration, no wire break.
Takes full effect after a **server deploy** and an **app release** (owner's step);
until then, updated senders already stop generating new phantom pushes even against
the old server (the old server simply ignores `silent`).

## Test plan

`server/tests/wake_pings.rs`, tagged `// spec: message-delivery/content-free-wake-pings`:
a silent `APPLICATION` envelope (a profile update) queued to an offline device fires
**0** wake pings; a following real message fires exactly **1**; a silent-only backlog
leaves the catch-up budget untouched (reuse the notification-reliability harness).
