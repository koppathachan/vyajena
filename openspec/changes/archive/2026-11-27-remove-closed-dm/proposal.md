## Why

A closed DM (its peer was removed from the server; direct-messages: DM closed on
user removal) stays in the chat list for good, read-only. The owner asked
(2026-11-23) to let the user remove it, deleting the thread and its stored
messages, and to have the removal reach the user's other devices. The app's
swipe on a closed-DM row already exists as a "coming soon" no-op.

This is the closed-DM slice of the drafted `delete-messages` change, shipped on
its own (owner, 2026-11-23, via the tracker). The rest of `delete-messages` (the
`Delete` body, tombstones, `Blobs.DeleteBlob`) is **not** in this change and
stays drafted.

## What Changes

- **A closed DM can be removed** from the chat list on a device: the core deletes
  the DM's chat row and its stored messages from SQLite, after telling the server.
  Offered only for closed DMs; an open DM, the lobby and a named group have no such
  action.
- **It syncs across the user's own devices** through a private per-user
  server-side *removed-conversation set* (owner mechanism (a), 2026-11-23): a new
  authenticated `Dms.RemoveClosedDm{group_id}` records the conversation id; on every
  connect the server delivers the caller's set to each of their devices as
  `ServerFrame.conversation_removed`, and each device removes the DM locally.
- **Privacy / invariants:** the server stores conversation ids only, never content
  or history (invariant: the server is the authority for routing, ordering and
  membership, never history). The set is readable and writable only by that user's
  own devices; nothing is sent to the gone peer or any other user. The swipe is
  behind a confirm dialog (a delete is never a bare swipe).
- **App:** the closed-DM swipe asks to confirm, then removes; the "coming soon"
  snackbar goes away.

## Open decisions resolved

- Sync mechanism: private per-user server marker, option (a) (owner, 2026-11-23).
- Slice shipped as its own change/PR (owner, 2026-11-23).

## Proto files touched

- `proto/vyajena/v1/dms.proto`: `Dms.RemoveClosedDm` (`RemoveClosedDmRequest`,
  `RemoveClosedDmResponse`).
- `proto/vyajena/v1/messaging.proto`: `ServerFrame.conversation_removed = 11`
  (`ConversationRemoved{ group_id }`); field 10 is already `group_closed`.
- Additive: `buf breaking` passes. `cargo run -p pbgen`.

## Capabilities

### New Capabilities

- `message-deletion`: introduced here with only the closed-DM requirements
  (Remove a closed DM; Removed-conversation sync). The drafted `delete-messages`
  change adds the rest of this capability; it must drop these two requirements
  from its own delta when this change is archived.

### Modified Capabilities

- `app-ui`: Remove a closed DM from the list.

## Impact

- **Server:** migration `0008_removed_conversations.sql` (`username`, `group_id`
  only); `Dms.RemoveClosedDm`; the set is sent in the connect catch-up.
- **Core:** `Client::remove_closed_dm(chat)`; applying `conversation_removed`.
- **Bridge/app:** `remove_closed_dm`; confirm dialog on the swipe.
- **Ops:** server upgrade before the app (an old server answers `UNIMPLEMENTED`;
  the app shows the error and keeps the DM). No existing data changes.
