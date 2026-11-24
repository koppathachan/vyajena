## ADDED Requirements

### Requirement: Who may delete
A delete SHALL be a `Content.Delete` body naming the target message's original
sender device and `msg_id`, carried in an `APPLICATION` envelope whose
`Envelope.deletes` names the same target. In the lobby and named groups only an
admin device SHALL delete a message — text or media (moderation); a regular member
SHALL NOT delete even their own message in a group. In a DM only the target's
original sender SHALL delete it, and the admin SHALL have no power. The server SHALL
authorise the delete from the authenticated sending device and reject an
unauthorised one with `REJECTION_REASON_POLICY_VIOLATION`, dropping nothing. A
client SHALL apply a received `Content.Delete` only when the delete's MLS sender is
so authorised, and SHALL ignore it otherwise. The server and the client both enforce
this.

#### Scenario: Admin deletes a message in a group
- **WHEN** the admin's device deletes Bob's message in a named group
- **THEN** the server accepts it and clients show "This message was deleted by the
  admin"

#### Scenario: Member cannot delete in a group, even their own
- **WHEN** Bob's device sends a delete for a message Bob sent in the lobby
- **THEN** the server rejects it with `REJECTION_REASON_POLICY_VIOLATION` and clients
  ignore it

#### Scenario: Sender deletes their own DM message
- **WHEN** Alice sends a delete for a message she sent in her DM with Bob
- **THEN** the server accepts it and Bob's client shows a tombstone in its place

#### Scenario: No one else deletes in a DM
- **WHEN** a delete for a message in the Alice–Bob DM arrives from any device other
  than the message's own sender
- **THEN** the server rejects it with `REJECTION_REASON_POLICY_VIOLATION` and clients
  ignore it

### Requirement: Tombstone, not erasure
A honoured delete SHALL leave the message in place as a tombstone, never erase it
from history. Clients SHALL show "This message was deleted" (and "by the admin" when
an admin device deleted another user's message) in the message's position, keeping
its order and any reply thread. The delete SHALL be a durable message like any
other; a client that predates the `Delete` body SHALL show the unknown-body
placeholder (message-content: Inner content format). The client enforces this.

#### Scenario: Deleted message keeps its place
- **WHEN** Alice deletes a message between two others
- **THEN** clients show a "This message was deleted" row in the same position, with
  the surrounding messages unmoved

#### Scenario: Old app sees a delete
- **WHEN** a client that predates the `Delete` body receives one
- **THEN** it shows the "needs a newer app version" placeholder and does not fail

### Requirement: Receiver may reveal in a DM
In a DM, a device that still holds the original of a deleted message SHALL show the
tombstone with a Reveal affordance that shows the original it kept; the message is
hidden by default and the receiver chooses whether to look. In the lobby and named
groups a tombstone SHALL offer no reveal. A device that never received the original
SHALL show the tombstone with nothing to reveal. The client enforces this.

#### Scenario: Bob reveals a deleted DM message
- **WHEN** Alice deletes a DM message Bob's phone already received, and Bob taps
  Reveal
- **THEN** Bob's phone shows the original it kept

#### Scenario: No reveal in a group
- **WHEN** a message is deleted in a named group
- **THEN** the tombstone offers no reveal on any device

#### Scenario: Nothing to reveal
- **WHEN** Carol was offline, the message was deleted before delivery, and Carol
  reconnects
- **THEN** Carol's phone shows the tombstone and offers no reveal

### Requirement: Deleting media and replies
Deleting a media message SHALL send the `Delete` body and SHALL immediately purge the
stored blob with `Blobs.DeleteBlob` (blob-store: Delete a blob), which removes the one
stored copy. The server SHALL authorise the purge as it authorises the message delete
(an admin of the blob's group, or the uploader in a DM). A message SHALL be deletable
whether or not its blob still exists; a purge of an absent blob SHALL be a safe no-op.
A device that holds the delete SHALL show any reply that quotes the deleted message
with a "deleted message" quote in place of the stored snapshot. The server and the
client enforce this.

#### Scenario: Delete an image purges the blob
- **WHEN** an authorised device deletes an image message
- **THEN** clients show a tombstone and the server purges the blob at once via
  `Blobs.DeleteBlob`

#### Scenario: Delete after the blob expired
- **WHEN** a media message whose blob the server already deleted is deleted
- **THEN** the delete is accepted, the `Blobs.DeleteBlob` purge is a no-op, and
  clients show a tombstone

#### Scenario: Reply to a deleted message
- **WHEN** Bob replied quoting Alice's message and Alice then deletes it
- **THEN** a device holding the delete shows Bob's reply with a "deleted message"
  quote
