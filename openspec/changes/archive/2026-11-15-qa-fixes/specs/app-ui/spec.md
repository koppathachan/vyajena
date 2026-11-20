## MODIFIED Requirements

### Requirement: Chat screen
The app SHALL show the group's messages from the core's history and event stream
in `server_seq` order, each other user's message with its sender shown by display
name and username (for example "Ali (alice)"), and each own message with its send
status: ⏳ sending, ✓ accepted, or failed with the reason. Messages from the
user's own devices (`own_user`) SHALL be right-aligned, sized to their text and
shown without a sender label; only messages this device sent (`outgoing`) SHALL
show a send status, inline in the bubble. A message whose body the core does not
know SHALL show a placeholder, and an `Unreadable` event SHALL show that a message
could not be read.

#### Scenario: Alice and Bob chat
- **WHEN** Alice sends "hi" and Bob replies "hey"
- **THEN** both see "hi" then "hey", and Alice sees "hi" go from ⏳ to ✓

#### Scenario: Own messages look like own
- **WHEN** Bob sends "hi" from his phone
- **THEN** his phone shows "hi" right-aligned, no wider than its text, with no
  "bob" label and a ✓ at the end of the bubble

#### Scenario: Message from the user's other device
- **WHEN** Bob's tablet sends "from the tablet"
- **THEN** Bob's phone shows it right-aligned with no label and no send status,
  and other members show it as "Bobby (bob)"

#### Scenario: Sent across a commit
- **WHEN** Alice's message is rejected with `STALE_EPOCH` and the core resends it
- **THEN** Alice sees one message going from ⏳ to ✓, never two

#### Scenario: Unknown body
- **WHEN** a message arrives with a content body this version does not know
- **THEN** the chat shows a placeholder saying the app needs an update

### Requirement: Notices in the chat
The app SHALL show in the chat, in order with the messages: a gap notice for
expired messages, a removed-for-inactivity notice with the time messages stop, a
rejoined notice, an identity-key-changed warning naming the user, a
removed-device notice, a group-rename notice naming who renamed it and the new
name, and membership notices (a user joined, a user added a device, a user removed
a device, the admin removed a user). The app SHALL NOT show membership notices for
inactivity removals or re-adds of the same device, nor for the commit that added
this device. The admin's device SHALL offer to accept a changed key, which calls
`accept_identity_key`.

#### Scenario: Back from holiday
- **WHEN** Bob opens the app after 10 days away and the core emits
  `RemovedForInactivity`, then `Rejoined`, then new messages
- **THEN** Bob's chat shows the removal notice with its time, the rejoined notice,
  and then the new messages

#### Scenario: Key changed
- **WHEN** the core emits `IdentityKeyChanged` for `bob@chat`
- **THEN** every member sees a warning in the chat that Bob's key changed

#### Scenario: Bob is removed
- **WHEN** the admin removes Bob and Bob's phone gets `Removed` from the core
- **THEN** Bob's chat shows "This device was removed from the group.", the
  indicator shows removed, and the composer is disabled

#### Scenario: Group renamed
- **WHEN** the admin renames the group from "Vyajena" to "Home chat"
- **THEN** every member sees "akhil renamed the group to “Home chat”" in the chat

#### Scenario: Membership changes
- **WHEN** Carol joins, then Bob links his tablet, then the admin removes Carol
- **THEN** members see "carol joined", "bob added a device" and "akhil removed
  carol", in that order

#### Scenario: Quiet inactivity re-add
- **WHEN** the admin's phone re-adds Bob's phone after an inactivity removal
- **THEN** other members see no membership notice for it

### Requirement: Foreground and background
The app SHALL start the core when it opens, SHALL let the core reconnect and catch
up when the app returns to the foreground or the phone's network comes back, and
SHALL show whether the core is connected.

#### Scenario: Back to the app
- **WHEN** Bob switches back to the app after an hour in another app
- **THEN** the core reconnects, catches up in order, and the connection indicator
  turns connected

#### Scenario: Network back
- **WHEN** Bob turns airplane mode off while the core is waiting to retry
- **THEN** the app calls `reconnect_now` and the core reconnects without waiting
  out its backoff
