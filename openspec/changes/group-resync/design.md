# Design — group-resync

## The field incident (evidence)

Deployed server `v0.4.0` / app `1.4.0+8`. A non-admin iPhone user could neither
send nor receive in the lobby: sends sat a silent hourglass forever, never
"failed", and no messages arrived. Android users and the admin's iPhone were
fine. The user was **not** intentionally removed/re-invited. A `main`↔`v0.4.0`
diff showed the send/accept/membership path is byte-identical (the reported
build is irrelevant to the mechanism), and an end-to-end test confirmed a
later-joiner and an existing member both send fine on `main`
(`client/tests/automatic_add.rs::a_third_joiner_and_the_earlier_member_both_send_in_the_lobby`).
So this is not the send path — it is a device **wedged behind the group epoch**.

## Why the hourglass is silent (not "failed")

`server/src/messaging/write.rs` checks the **strict epoch rule before** the
active-member check: a send whose envelope epoch ≠ the group's current epoch is
rejected `STALE_EPOCH` *before* membership is even considered. The core treats
`STALE_EPOCH` as "catch up and resend with the same `msg_id`" and **holds** the
message (`client/src/engine/outbox.rs::flush_outbox`, held while
`epoch < server_epochs[group]`). So a behind device — even one already removed
from the group — gets `STALE_EPOCH`, never `POLICY_VIOLATION`, and its message
stays `sending` indefinitely. `POLICY_VIOLATION` (which would show "failed") is
only reachable by a device that is *at the current epoch but not a member*, which
a wedged device never is.

## How a device gets wedged

`group-membership`: Inactive devices are kept out — a device offline 7 days, or
whose queue fills, is marked inactive: the server **deletes its queued entries**
and removes its leaf. iOS background suspension makes this ordinary. The device
keeps its local group at the old epoch. On reconnect, the designed recovery
(Automatic re-add) re-adds it *if* a device of its user or an admin device is
online to do so; that path is correct and tested (`client/tests/inactivity.rs`,
including "a message typed while removed goes out after the re-add"). The orphan
is the case where that recovery does not complete.

## The three gaps (what makes it permanent and silent)

- **G1 — no self-request while holding a stale group.** The startup re-add
  request (`client/src/engine/mod.rs`) is gated on `store.group().is_none()`. A
  wedged device holds a *stale* group, so it never self-requests on restart; a
  force-quit + reopen does nothing. Its only other self-request path is
  `refuse_welcome` → `RequestReadd`, and an orphan receives no Welcome to refuse.
- **G2 — nothing surfaced, no action.** Holding a group puts the device past the
  group-gate "waiting"/"re-add limit" UI (`app/lib/chat/group_gate.dart`), so the
  only signals that exist render nowhere. The held send is a silent hourglass with
  no terminal state and no re-sync control.
- **G3 — silent removal.** The `RemovedNotice` that tells a device it was removed
  for inactivity (shown as an in-chat notice) is delivered best-effort; a device
  offline at the moment of removal can miss it entirely and never learn.

## Decisions

- **D1 — detect the wedge client-side, then self-request.** The signal is: an own
  message held behind `server_epochs[group]` **and** the stream caught up (acked to
  the server's latest) while still behind. At that point the epoch-advancing commit
  will never arrive (the device is no longer a current member), so the core issues
  `Keys.RequestReadd` (reason "behind epoch"), bounded by the existing
  `MAX_READD_REQUESTS_PER_DAY`. Chosen over blindly dropping the stale group at
  startup, which would wrongly fire for a device that is merely transiently behind
  and still catching up. Reuses existing server machinery; no wire change.
- **D2 — terminal re-sync state, never a silent hourglass.** When re-add cannot
  complete (no re-adder online, or the daily limit is hit), the core emits an
  out-of-sync event and the app shows an in-chat "out of sync — re-sync" banner
  with a **Re-sync** button (re-issues the request). This satisfies the invariant
  *nothing is dropped silently*. Chosen over marking the message plain "failed",
  which would wrongly imply the text is lost and un-resendable.
- **D3 — durable removal notice (server).** The server records that an inactive
  device owes a removal notice and delivers it on the device's next connect, so
  removal is never silent even if the device was offline when removed. A small
  server-side record (SQLite migration `0010+`); no wire field (reuses
  `RemovedNotice`).
- **D4 — admin recovery is documented and reachable.** For a wiped/unrecoverable
  device the only path is admin-driven: `Admin.RemoveUser` first (it frees the
  username; `Admin.CreateInvite` refuses a username whose user still exists and is
  not removed — `server/src/admin/mod.rs`), then `Admin.CreateInvite`, then the
  device re-registers on the removed-user reset path
  (`server/src/devices/mod.rs`, present in `v0.4.0`) and is auto-added back to the
  lobby by the admin's device (`group-membership`: Automatic add of new users).
  Re-adding to a *named* group has no UI today (the Members screen is view-only) —
  tracked as a follow-up, out of scope here.

## Open decision for the owner — admin single-device lockout

`Admin.CreateInvite`'s bootstrap admin invite is minted only when the server has
**zero** users. So if the sole admin device is wiped, the admin can never
re-bootstrap — a hard lockout. This overlaps `replace-user` Phase 2 (co-signed
succession). This change does not solve it; it recommends, and documents, that the
admin **link a second admin device now** as a standing safeguard, and flags the
real fix (an admin-recovery mechanism) to the owner as a `replace-user` Phase 2
decision.

## Interactions

- `notification-reliability` (#27) redelivers undelivered *envelopes* on reconnect;
  it does not help once the epoch-advancing commit was deleted with an inactive
  device's queue. group-resync operates at the MLS-epoch level above it.
- `replace-user` covers the removed-user reset and the admin succession; this change
  reuses the reset and defers succession to it.
- Strict epoch (`max_past_epochs = 0`) is unchanged; recovery is re-add + Welcome
  (a fresh start), never relaxing the epoch rule.
