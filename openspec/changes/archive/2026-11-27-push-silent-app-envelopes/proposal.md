## Why

A user (and the owner) still get a push with nothing new to read, **after**
`push-kind-aware` (#46) shipped. That change made the wake push kind-aware: an
offline device is only pushed when it has a pending `APPLICATION` entry, so MLS
control envelopes (`COMMIT`/`WELCOME`/`PROPOSAL`) no longer alert. But three
`Content` bodies are `APPLICATION` **on the wire** yet carry no user-visible
message, and the receiving core applies them silently:

- **`Profile`** — a user's display name / "about". `Client::set_profile` sends it
  "for every member", so one edit fans out an `APPLICATION` envelope to every DM
  peer and every group member; it is also auto-re-sent when a commit adds a device
  (qa-fixes D2). The receiver renders nothing (`Decoded::Profile => None`).
- **`GroupSettings`** — an admin's group rename, broadcast to all members; the
  receiver renders nothing (`Decoded::GroupSettings => None`).
- **`Delete`** — a message-deletion tombstone; no new message (`revealable:
  false`).

Because all three are `KIND_APPLICATION`, `has_application_backlog` is true and the
server fires a **visible** wake push, but the app opens to nothing new — the exact
"notification, no message" complaint. Profile updates are frequent and broadcast to
everyone, which is why both a user and the owner saw it.

The server only ever sees ciphertext, so it cannot tell these apart from a real
message by decrypting. It needs a **plaintext hint** on the envelope — the same
shape as the existing `Envelope.deletes` plaintext field. The deleting client
already sets `deletes`, so the server could suppress deletes today; profile and
group-settings envelopes have no such signal.

(Not the cause, checked: a deactivated/deleted account evicts the user via MLS
**Remove proposals/commits** — control, already filtered — plus DM-close
`ServerFrame`s that are never pushed; admin remove/close-group likewise. Those
paths are already silent.)

## What Changes

- **A new plaintext `Envelope.silent` flag (proto).** The sending client sets
  `silent = true` on an `APPLICATION` envelope whose `Content` carries no
  user-visible message — a `Profile`, a `GroupSettings` or a `Delete`. It is unset
  (false) on a text/image/audio/video message, and on every control envelope. A
  server that predates the field reads the default (false) and behaves as today.
- **The wake push becomes content-aware, not just kind-aware.** The server SHALL
  send the content-free wake push to an offline device only when it has a pending
  `APPLICATION` entry that is **not** `silent`. A backlog of only control and/or
  silent application envelopes fires no push; those entries stay queued and sync on
  the device's next `Connect`.
- **The sweep catch-up ping is content-aware too.** `backlogged_devices` only
  considers devices with a pending **notifiable** (`APPLICATION`, not `silent`)
  entry, so a silent-only backlog neither fires a catch-up ping nor spends the
  device's catch-up budget.
- **Queue cap and TTL are unchanged.** A silent application envelope still counts
  toward `application_queue_len` (the per-device cap) and still expires on the
  application TTL — it is real queue load; only the *push* is suppressed.

This refines the `message-delivery` "Content-free wake pings" requirement that
`push-kind-aware` last modified, so it goes through OpenSpec and is owner-reviewed
before merge.

## Open decisions resolved

- A silent application envelope (profile / group settings / message delete) fires
  no visible push and no catch-up ping; it syncs on reconnect — consistent with the
  owner's visible-only-push, notify-only-real-messages stance (2026-11-16).
- The signal is a sender-set plaintext boolean (`silent`), default false, so the
  change is backward compatible: old clients and old servers keep today's behavior,
  and the fix takes effect for an envelope as soon as the *sender* is updated.

## Proto files touched

- `proto/vyajena/v1/messaging.proto`: add `bool silent = 12;` to `Envelope`
  (field 12 is the next free client-set number; 6 is reserved, 10/11 used).
  Regenerate with `cargo run -p pbgen`. Additive, non-breaking (`buf breaking`).

## Capabilities

### Modified Capabilities

- `message-delivery`: the content-free wake push and the sweep catch-up ping are
  sent only for a **notifiable** backlog (`APPLICATION` and not `silent`); a
  backlog of only control and/or silent application envelopes fires neither.

## Impact

- **Proto:** add `Envelope.silent` (field 12); regenerate `pb.rs` files.
- **Client:** `messaging::is_silent_content` (true for `Profile`/`GroupSettings`/
  `Delete`); `engine::outbox` sets `silent` on the `APPLICATION` envelope it
  builds. No UI change.
- **Server:** migration `0013_silent_envelopes.sql` adds `envelopes.silent`;
  `log::insert_envelope` persists it; `log::has_application_backlog` and
  `log::backlogged_devices` add `AND e.silent = 0`. `notify_in` and the sweep are
  otherwise untouched. `application_queue_len` and `expire` unchanged.
- **Tests:** `server/tests/wake_pings.rs` — a silent application envelope (a
  profile update) for an offline device fires no wake ping, while a real message
  does; a silent-only backlog spends no catch-up budget.
- **Ops:** server-only deploy plus a new app build; **needs a server deploy and an
  app release** to fully take effect (owner's step). Existing data unchanged
  (`silent` defaults to 0 for existing rows).

## Relationship to other changes

- Builds on `push-kind-aware` (Complete, #46) and refines the same requirement.
  **Archive ordering:** archive `push-kind-aware` before (or together with) this
  change; this change's MODIFIED requirement is written as the full end state
  (kind- and content-aware), so it supersedes the kind-aware-only text.
- The held `smarter-notifications` proposal generalizes this (type-without-decrypt,
  local notifications). This change is the narrow, shippable bug fix; it does not
  depend on that larger design and is compatible with it.
