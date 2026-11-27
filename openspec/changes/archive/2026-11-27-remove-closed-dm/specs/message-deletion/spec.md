## ADDED Requirements

### Requirement: Remove a closed DM
A user SHALL be able to remove a closed DM — one whose peer was removed from the
server (direct-messages: DM closed on user removal) — from their chat list. The
removal SHALL delete the DM thread and its locally stored messages from the device,
and SHALL sync to the user's own other devices so the DM leaves the chat list on all
of them (owner, 2026-11-23). It SHALL NOT notify or send a tombstone to the removed
peer, who is gone, and SHALL NOT reach any other user. It SHALL apply only to a
closed DM; an open DM SHALL be unaffected and keeps the sender-own,
reveal-able-tombstone model (Who may delete). The removal SHALL call
`Dms.RemoveClosedDm` to record the DM in the user's private server-side removed set
(Removed-conversation sync). The client enforces the local delete.

#### Scenario: Remove a dead conversation
- **WHEN** Bob was removed from the server, Alice's DM with Bob is closed, and Alice
  removes it on her phone
- **THEN** the DM thread and its stored messages are gone from Alice's phone, the DM
  leaves the chat list on Alice's other devices too, and nothing is sent to the
  removed peer or any other user

#### Scenario: Open DM is not removable this way
- **WHEN** Alice's DM with Carol is open because Carol is still a member
- **THEN** no remove affordance is offered for it; only per-message delete applies

### Requirement: Removed-conversation sync
The server SHALL keep a private per-user removed-conversation set. `Dms.RemoveClosedDm`
SHALL add a closed DM's `group_id` to the caller's user's set, idempotently, and SHALL
be accepted only from a device of a user that was a member of that now-closed DM,
rejecting any other caller with `PERMISSION_DENIED`. On connect the server SHALL
deliver that user's set to each of their devices as
`ServerFrame.conversation_removed{group_id}` entries, and each device SHALL remove the
named DM locally. The server SHALL store only the conversation ids, never message
content or history, and SHALL expose a user's set to that user's own devices alone.
The server enforces this.

#### Scenario: Removal reaches the other device
- **WHEN** Alice removes a closed DM on her phone and her tablet later connects
- **THEN** the server sends the tablet `conversation_removed` for that `group_id` and
  the tablet removes the DM locally

#### Scenario: Only your own set
- **WHEN** Bob's device connects
- **THEN** it never receives Alice's removed set and cannot write to it

#### Scenario: Idempotent removal
- **WHEN** `Dms.RemoveClosedDm` names a conversation already in the user's removed set
- **THEN** the set is unchanged and the call succeeds

#### Scenario: Only a closed DM of the caller
- **WHEN** Alice's device calls `Dms.RemoveClosedDm` for an open DM, for the lobby, or for Bob and Carol's closed DM
- **THEN** the server rejects it with `PERMISSION_DENIED` and Alice's set is unchanged

#### Scenario: A frame never removes an open chat
- **WHEN** a device receives `conversation_removed` for a chat it still has open
- **THEN** it ignores the frame and keeps the chat
