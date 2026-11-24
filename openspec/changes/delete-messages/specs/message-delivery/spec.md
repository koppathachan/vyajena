## ADDED Requirements

### Requirement: Deletion retracts undelivered copies
The server SHALL, on accepting an `APPLICATION` envelope whose `Envelope.deletes`
names a target and only after authorising the delete (message-deletion: Who may
delete), find the target from `(group_id, target_sender, target_msg_id)`, delete the
target's `APPLICATION` queue entries for every device of the group that has not yet
acknowledged them, and then deliver the Delete envelope to every member device as
usual. A device that had not yet been delivered the target SHALL therefore never
receive its plaintext, only the delete. The server SHALL accept the delete even when
no such target exists. This retract is the one case in which the server removes an
accepted, undelivered `APPLICATION` entry, and it SHALL always be paired with the
delivered delete, so no message is dropped silently (invariant: Accepted means
durable). The server enforces this.

#### Scenario: Offline receiver never gets the original
- **WHEN** Alice deletes a message while Carol's phone is offline and has not acked
  it
- **THEN** the server deletes that queued entry for Carol, and on reconnect Carol
  receives the delete but never the original plaintext

#### Scenario: Online receiver gets the delete over the original
- **WHEN** Bob already received the message and Alice deletes it
- **THEN** Bob receives the delete and his client shows a tombstone over his stored
  copy

#### Scenario: Delete of an unknown target
- **WHEN** the deleted message had already expired from the queues
- **THEN** the server still accepts the delete and delivers it

#### Scenario: Racing acknowledgement
- **WHEN** Carol acks the target at the same moment the delete lands
- **THEN** the retract finds the entry already gone and the delete is still
  delivered to Carol
