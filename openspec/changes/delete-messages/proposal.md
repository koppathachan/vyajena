## Why

The architecture doc's Scope says "no edit or delete". The owner asked
(2026-11-20) to add deleting a message. In a group the admin is a moderator and a
sender owns their own messages; in a DM both users have equal rights and delete is
"consent-based": the sender owns the message, and the receiver can choose to see it
or not.

The owner sharpened the DM semantic (2026-11-20): **a delete is a *real* delete
only while the message is still undelivered** — if it has not reached the
receiver's device yet, the server drops the queued copy and the receiver never
sees it. **Once it has reached the device**, the device already holds the
plaintext; the delete becomes a request the receiver's client shows as a tombstone
and **may reveal**. This keeps the invariant "accepted means durable / nothing is
dropped silently": a delete never erases history — it is a superseding Delete
message that every device receives, so each one learns a message was deleted.

## What Changes

- **A new `Delete` body in the `Content` oneof** (owner, 2026-11-20). It names the
  message it deletes (that message's original sender device and `msg_id`, as a
  `ReplyTo` already names a reply's target). Old clients already degrade an unknown
  body to the "needs a newer app" placeholder (message-content: Inner content
  format), so a delete reaching a pre-delete app shows as an unknown message, not a
  failure.
- **A delete is a superseding message, never erased history** (owner invariant
  "accepted means durable"). The original row stays; a client that honours the
  delete shows a tombstone **"This message was deleted"** (and "by the admin" for a
  moderator delete) in its place (owner, 2026-11-20 — tombstone, not vanish).
- **Who may delete** (owner, 2026-11-20; narrowed 2026-11-23):
  - In the **lobby and named groups**: **only an admin device** may delete a
    message — text or media — as moderation (owner, 2026-11-23). A regular member
    may **no longer** delete their own message in a group.
  - In a **DM**: each user may delete **their own** message only ("equal rights",
    "the sender owns it"); the admin has no power in a DM (unchanged).
  - **Both the server and the client enforce it**: the server authorises the delete
    from the stream metadata (an admin of this group, or — in a DM — the original
    sender) and drops the undelivered copies; the client honours the encrypted
    Delete only from an MLS-authenticated authorised sender.
- **Real delete before delivery** (owner, 2026-11-20): a delete envelope carries,
  in server-visible metadata, the target it deletes. On accepting it the server
  **removes the target's still-queued `APPLICATION` copies** for every device that
  has not acked them, in that group, then delivers the Delete envelope. A device
  that had not yet received the original never gets its plaintext — only the
  tombstone.
- **Receiver may reveal, in a DM only** (owner, 2026-11-20): a DM device that
  already held the original shows the tombstone with a **Reveal** affordance that
  shows the original it still has. A lobby or named-group delete offers no reveal.
- **Media** (owner, 2026-11-23 — this reverses the 2026-11-20 "no blob RPC"):
  deleting a media message sends the superseding `Delete` **and immediately purges
  the single stored blob** on the server through a new authenticated
  **`Blobs.DeleteBlob`** RPC (one copy per `blob_id`), instead of waiting for
  ack-delete + 7-day expiry. The server authorises the purge with the **same
  authority as the message delete** (an admin in a group; the blob's uploader in a
  DM). A message may still be deleted **whether or not its blob still exists** — a
  purge of an already-expired or already-deleted blob is a safe no-op.
- **Replies to a deleted message:** a device that holds the delete SHALL show a
  reply quoting the deleted message with a "deleted message" quote, so the quote
  snapshot does not keep showing the deleted text.
- **Closed-DM removal is a separate change (not here).** Removing a closed DM from
  the chat list, with cross-device sync, was carved out of this change and ships on
  its own as the `remove-closed-dm` change (owner, 2026-11-23). Its
  `Dms.RemoveClosedDm` RPC and `ServerFrame.conversation_removed` frame live there,
  not in delete-messages. This change is only the per-message delete (text and media
  tombstones, the DM reveal, `Blobs.DeleteBlob`).
- **Delete only** (owner, 2026-11-20): editing a message is **not** in this change;
  it is a later change that can reuse the supersede machinery.
- **Push:** unchanged, content-free "New message"; a delete wakes devices like any
  message.
- **Out of scope:** editing; deleting a whole chat; deleting another user's message
  in a DM; undo/trash.
- Details and decisions: `design.md`.

## Open decisions resolved (owner, 2026-11-20)

- **DM semantic:** a delete is a real removal only while the message is still
  queued undelivered; once delivered it is a tombstone the receiver may reveal.
- **Display:** a tombstone "This message was deleted", not a vanished row.
- **Media (updated 2026-11-23):** an authenticated `Blobs.DeleteBlob` immediately
  purges the one stored blob on a media delete; deleting after the blob expired is
  still allowed (no-op purge).
- **Group delete (updated 2026-11-23):** in the lobby and named groups delete is
  **admin-only** (moderation), for text and media (media confirmed admin-only by the
  owner 2026-11-23); a member no longer deletes their own in a group. DMs are
  unchanged (each user deletes only their own).
- **Scope:** delete only, no edit; closed-DM removal moved to the `remove-closed-dm`
  change (owner, 2026-11-23).
- **Enforcement:** server (authorise + retract undelivered) and client (honour the
  encrypted Delete; tombstone; DM reveal).

## Invariant interaction (owner-approved, 2026-11-20)

- **"Accepted means durable / nothing dropped silently"** gains a clarification: an
  accepted `APPLICATION` message MAY be retracted from the undelivered queues of
  other devices **by an admin in the lobby or a named group, or by the original
  sender in a DM**, and such a retract is always accompanied by a delivered Delete tombstone,
  so the message is superseded, never silently lost. `docs/architecture-decisions.md`
  (Scope "no edit or delete" → "delete, no edit") and `openspec/config.yaml`
  context are updated in tasks.

## Proto files touched

- `proto/vyajena/v1/content.proto`: a new `Delete` body in the `Content` oneof,
  naming the deleted message's original sender device and `msg_id`. The field number
  is **TBD** — assign the next free `Content` number in merge order and coordinate
  with the other pending `Content` changes (`audio-messages` `Audio`,
  `video-messages` `Video`, `audio-calls` `CallSignal`) so no two bodies share a
  number; `buf breaking` catches a clash.
- `proto/vyajena/v1/messaging.proto`: a new server-visible `Envelope.deletes`
  (`DeleteTarget{ target_sender, target_msg_id }`), so the server can authorise the
  delete and drop the target's undelivered copies without seeing content. Field
  number **TBD** (coordinate with `remove-closed-dm`'s `ServerFrame` additions and
  the other pending messaging changes).
- `proto/vyajena/v1/blobs.proto`: a new authenticated `Blobs.DeleteBlob` RPC that
  immediately purges one stored blob (added 2026-11-23); `cargo run -p pbgen`.
- `proto/vyajena/v1/common.proto`: `REJECTION_REASON_POLICY_VIOLATION` is reused
  for an unauthorised delete (no change).
- `protocol/`: the delete authorization rule — an admin in the lobby or a named
  group; in a DM only the target's sender.
- All additive: `buf breaking` passes.

## Capabilities

### New Capabilities

- `message-deletion`: the `Delete` body, who may delete, the tombstone, the DM
  reveal, deleting media and replies.

### Modified Capabilities

- `message-content`: the `Content` oneof gains `Delete`.
- `message-delivery`: a delete retracts the target's undelivered queue copies.
- `blob-store`: a new `DeleteBlob` RPC purges the stored blob immediately on a
  media delete.
- `app-ui`: a Delete action, the tombstone, and the DM reveal.

## Impact

- **Server (`server/`):** on an `APPLICATION` envelope with `deletes` set,
  authorise it and delete the target's queue entries for devices that have not
  acked, then deliver it as usual. On a media delete it also handles
  `Blobs.DeleteBlob`, authorising the purge (an admin in a group; the blob's
  uploader in a DM) and removing the one stored blob; it records the uploading
  device per blob so a DM purge can be authorised. No store migration for the queue
  (entries already keyed by group, device, `server_seq`); a small addition records
  the blob uploader.
- **Core (`client/`):** a `Delete` body; mark the target message deleted in the
  store (keep the original plaintext for a DM reveal); set `deletes` on the sent
  envelope; the per-chat delete-right check; a `delete_message(chat, id)` API; a
  `reveal` for a DM; events for a message becoming deleted.
- **Bridge and app:** a Delete action in the long-press sheet; the tombstone
  bubble; the DM Reveal affordance; a `ChatBody::Deleted` (with the original kept
  for a DM reveal).
- **Ops:** no server data migration; server upgrade before the app update so the
  server honours `deletes` (an old server ignores the field, so a delete would only
  tombstone, never retract — the app can gate the action on a server that knows it,
  as with DMs and named groups; see design D7).
- **Depends on:** `media-blobs` (image deletes), `dms` (DM rules) and
  `lobby-multi-group` (lobby/named-group admin rules) for the full matrix; the core
  mechanism stands alone.
