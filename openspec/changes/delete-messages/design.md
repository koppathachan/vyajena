## Context

- **The `Content` oneof already degrades unknown bodies** (message-content: Inner
  content format): a client that predates `Delete` shows the "needs a newer app"
  placeholder, so adding a body is safe for old apps.
- **A message already names another by `(DeviceRef sender, msg_id)`.** `ReplyTo`
  does exactly this. A `Delete` reuses that shape to name its target.
- **The server already tracks `(sender device, msg_id) → server_seq`** for
  idempotent sends (message-delivery: Idempotent sends), and per-device queue
  entries keyed by group and `server_seq` (Per-device queues). So the server can
  find a target's queue entries from the target's sender device and `msg_id`.
- **The server never sees content.** A `Content.Delete` is encrypted; the server
  cannot read which message it deletes unless the envelope also carries that in
  plaintext metadata.
- **Owner decisions (2026-11-20):** DM delete is real only while undelivered, else
  a reveal-able tombstone; tombstone not vanish; media via ack-delete/expiry,
  delete anytime; delete only, no edit; server and client both enforce.

## Goals / Non-Goals

**Goals**
- Delete a message as a superseding, durable event — never erased history.
- A real removal for a receiver who has not yet been delivered the message.
- A tombstone for a receiver who already has it, with a DM-only reveal.
- Server and client both enforce who may delete.

**Non-goals**
- Editing; deleting a chat; deleting another user's message in a DM; undo.

## Decisions

### D1. The `Content.Delete` body is the client's source of truth

- **`Delete { DeviceRef target_sender; bytes target_msg_id; }`** names the message
  it deletes, the same way `ReplyTo` names a reply's target. It is the encrypted,
  MLS-authenticated statement that message Y is deleted; a receiving client honours
  it only if the delete's own MLS sender is authorised (D3).
- **It supersedes, it does not erase.** The original message stays in the store;
  the client marks it deleted and shows a tombstone (D5).
- **Why in `Content`:** a delete is a durable chat event that must reach every
  device in order and survive restarts, like every other message; old apps degrade
  it cleanly.

### D2. The server-visible `Envelope.deletes` drives the retract

- **`Envelope.deletes = DeleteTarget{ DeviceRef target_sender; bytes target_msg_id }`**
  is set by the deleting client on the `APPLICATION` envelope that carries the
  `Content.Delete`. The server reads it (plaintext metadata), authorises the delete
  (D3) and drops the target's undelivered copies (D4). The server never reads the
  encrypted `Content`.
- **Two copies of the target, on purpose:** clients trust only the encrypted
  `Content.Delete` (a server cannot forge an MLS message), so a server tampering
  with `deletes` cannot fake a delete on any device — it can only drop queue
  entries, which it already controls. The plaintext copy exists solely so the
  server can act before delivery.
- **The server knows it is a delete** by `deletes` being set; application
  envelopes are otherwise opaque.

### D3. Who may delete, and both sides enforce it (narrowed 2026-11-23)

- **Lobby and named groups:** the deleter must be an **admin device** of that group,
  for text and media alike (moderation). A regular member may **not** delete even
  their own message in a group (owner, 2026-11-23).
- **DM:** the deleter must be the target's original sender. The admin is not a DM
  member and has no say.
- **Server enforcement:** on an envelope with `deletes` set, the server authorises
  it — an admin device in a lobby/named group, or (in a DM) the target's sender
  device — and otherwise rejects with `REJECTION_REASON_POLICY_VIOLATION`, dropping
  nothing.
- **Client enforcement:** a client applies a `Content.Delete` only if the delete
  message's MLS sender is an admin device (lobby/named group) or the target's sender
  (DM); it ignores an unauthorised one (a misbehaving peer).
- **The server does not require the target to still exist:** a delete of an expired
  or already-gone message is accepted and simply tombstones (owner: delete
  anytime).

### D4. Real delete while undelivered; the durability invariant holds

- **On accepting a delete envelope**, the server finds the target's `server_seq`
  from `(group_id, target_sender, target_msg_id)` and **deletes that `APPLICATION`
  queue entry for every device of the group that has not yet acked past it**. Those
  devices never receive the original plaintext.
- **It then delivers the Delete envelope** to every member device as usual. A
  device that never got the original gets only the tombstone; a device that already
  had it shows the tombstone over its stored copy.
- **"Accepted means durable / nothing dropped silently" still holds:** the retract
  is authorised (D3) and is always paired with a delivered Delete tombstone, so no
  message is lost without the receiver learning a message was deleted. This is the
  owner-approved clarification recorded in the proposal and the arch doc.
- **Spike S1 — done 2026-11-20, by reading `server/src/store/migrations/0003_messaging.sql`
  and `0005_groups.sql`:** the retract is feasible with no schema change and no
  cursor/ordering breakage.
  - `dedup(sender_device_id, msg_id) → server_seq` maps `(target_sender,
    target_msg_id)` to the target's `server_seq`; the group is on the envelope.
  - The undelivered copies are exactly the rows left in
    `deliveries(device_id, server_seq)` for that `server_seq` with `welcome = 0`,
    because a device's delivery row is deleted when it acks (so an already-acked —
    already-delivered — device has no row and is untouched). The retract is one
    statement: `DELETE FROM deliveries WHERE server_seq = <target_seq> AND welcome
    = 0`.
  - `cursors.acked_seq` is a high-water mark, not a per-row structure, so deleting
    a mid-sequence delivery row never breaks a cursor; catch-up (deliveries above
    `acked_seq`) simply no longer lists the target.
  - **Racing ack:** if a device acks the target exactly as the delete lands, its
    delivery row is already gone, the `DELETE` matches zero rows, and the delete
    envelope is still queued — harmless.
  - **Expired target (`dedup` row past its 7-day keep):** the server cannot look
    up the original sender, but a self-delete is still provable (the authenticated
    device claims itself as `target_sender`) and an admin may delete anything in a
    group, so authorization still holds; there is simply nothing left to retract,
    only the tombstone to deliver.

### D5. Tombstone, not vanish; reveal only in a DM

- **A honoured delete shows a tombstone** "This message was deleted", with "by the
  admin" when an admin device deleted someone else's message. The row stays so
  reply threads and ordering are intact (owner: tombstone, not vanish).
- **A DM device that still holds the original** shows a **Reveal** affordance; on
  tap it shows the original it kept. This is the owner's "the receiver can choose
  to see or not" — the delete is honoured by default (hidden) but the receiver may
  look. A lobby or named-group delete offers no reveal.
- **The core keeps the original plaintext** for a DM so Reveal can work; for a
  lobby/named-group delete it may drop the plaintext (no reveal), keeping only the
  tombstone. The sending device keeps enough to show its own tombstone.

### D6. Media: immediate blob purge, and replies (updated 2026-11-23)

- **Deleting a media message** sends the `Delete` body and, additionally, purges the
  stored blob at once with a new `Blobs.DeleteBlob` RPC — reversing the 2026-11-20
  "rely on ack-delete + expiry" decision (owner, 2026-11-23). The server holds **one
  copy per `blob_id`**, and the purge removes it.
- **Authorisation mirrors the message delete:** the server authorises `DeleteBlob`
  for an **admin** of the blob's group (lobby/named group), or the blob's
  **uploader** in a DM. To authorise a DM purge the server records the uploading
  device per blob (a small blob-metadata addition). The client calls `DeleteBlob`
  after sending the authorised `Delete`.
- **Deleting after the blob expired or was already deleted** is fine: `DeleteBlob`
  on an absent blob is a safe no-op, and the message delete does not depend on the
  blob existing.
- **A reply quoting a deleted message:** a device that holds the delete SHALL show
  that reply's quote as "deleted message" instead of the stored snapshot, so the
  quote does not keep surfacing the deleted text. A device that has not seen the
  delete still shows the old quote (best effort; the quote is a 100-char snapshot,
  not the full message).

### D7. No new protocol version; server-first deploy covers the retract

- **The change is additive** and old apps degrade the body. No new group-context
  extension or protocol version is needed.
- **Against a server that does not yet know `deletes`,** a delete would still
  tombstone on honouring clients but would not retract undelivered copies. Because
  the server is deployed before the app update (as every change here is), by the
  time the delete action ships the server honours `deletes`. The app therefore
  needs no capability gate for delete.

```mermaid
sequenceDiagram
    participant A as Alice (sender)
    participant S as server
    participant B as Bob online
    participant C as Carol offline
    A->>S: APPLICATION {Content.Delete(target)} , Envelope.deletes=target
    S->>S: authorise (Alice is the target's sender)
    S->>S: drop target's queued APPLICATION entry for Carol (unacked)
    S-->>A: accepted
    S->>B: Delete envelope  (tombstone; DM: Reveal)
    Note over C: never receives the original plaintext
    C->>S: (reconnect) catch-up
    S->>C: Delete envelope only  (tombstone, nothing to reveal)
```

## Spikes (before code that relies on them)

- **S1 — done 2026-11-20 (by reading the store schema), result in D4:** the retract
  is `DELETE FROM deliveries WHERE server_seq = <target_seq> AND welcome = 0`, with
  `<target_seq>` from the `dedup` table; no cursor/ordering breakage; racing ack
  harmless. No runtime spike needed.

## Risks / Trade-offs

- **A modified client can ignore a delete.** E2EE cannot force erasure on a device
  that already holds the plaintext; the honest promise is "real before delivery,
  tombstone after". The UI and docs say so plainly.
- **The server learns the delete linkage** (which msg_id deletes which) from
  `deletes`. It already sees msg_ids, senders and timing; this is a small, accepted
  addition.
- **Reply quotes can lag** on a device that missed the delete; bounded to a 100-char
  snapshot.
- **The retract touches the durability invariant.** Scoped tightly (authorised,
  always paired with a tombstone) and recorded as an owner-approved clarification.

## Migration Plan

1. Deploy the server (it begins honouring `deletes`). Old apps keep working; a
   delete from a new app tombstones everywhere and retracts undelivered copies.
2. Release the app (owner step): the Delete action and the tombstone/Reveal UI.

**Rollback:** the server rollback stops honouring `deletes`; deletes already
delivered stay as tombstones on clients (durable in the message store).

### D8. Closed-DM removal is a separate change

- **Removing a closed DM** from the chat list, with cross-device sync via a private
  per-user server-side removed-conversation set (`Dms.RemoveClosedDm` +
  `ServerFrame.conversation_removed`), was **carved out of delete-messages** and
  ships on its own as the `remove-closed-dm` change (owner, 2026-11-23). It is not
  designed here; see that change for its decisions. This change covers only the
  per-message delete (tombstones, the DM reveal, `Blobs.DeleteBlob`).

## Open Questions (owner)

1. **Reveal persistence:** once a DM receiver reveals, does it stay revealed on
   that device, or re-hide on reopen?
2. **Admin delete attribution:** show "deleted by the admin", or just "deleted"?
3. **Bulk delete / delete-for-me:** a later change?
4. **Group media delete = admin-only — RESOLVED (owner, 2026-11-23):** admin-only,
   the same as group text; a member does not delete their own media in a group.
