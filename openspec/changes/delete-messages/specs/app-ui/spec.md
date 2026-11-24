## ADDED Requirements

### Requirement: Deleting a message
The long-press sheet SHALL offer a Delete action in the lobby and named groups only
on an admin device (on any message), and in a DM only on the user's own messages.
Choosing it SHALL ask the core to delete the message. A deleted message SHALL show as
a tombstone "This message was deleted" (and "by the admin" when an admin deleted a
message in a group), keeping its place in the list. In a DM, a tombstone whose
original this device still holds SHALL offer Reveal, which shows the original; lobby
and named-group tombstones SHALL offer no reveal. The app enforces this.

#### Scenario: Admin delete in a group
- **WHEN** the admin long-presses Bob's message in a named group and taps Delete
- **THEN** clients show "This message was deleted by the admin"

#### Scenario: Member has no delete in a group
- **WHEN** Bob, a non-admin, long-presses his own message in the lobby
- **THEN** the sheet offers no Delete action

#### Scenario: Delete own DM message
- **WHEN** Alice long-presses her own message in her DM with Bob and taps Delete
- **THEN** the core deletes it and Bob's chat shows "This message was deleted" in its
  place

#### Scenario: Reveal in a DM
- **WHEN** Bob's DM shows a tombstone for a message he still holds and he taps
  Reveal
- **THEN** the original is shown
