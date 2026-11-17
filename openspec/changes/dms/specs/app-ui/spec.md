## ADDED Requirements

### Requirement: Chat list
The app SHALL show a chat list with the main group first, then each DM by the
peer's display name, newest message first, each with an unread mark and the
number of unseen mentions of the user; closed DMs SHALL be shown last and
greyed. Tapping a chat SHALL open it. When the user has at least one DM the app
SHALL open on the chat list; with none it SHALL open on the group's chat as
before. A DM that arrives while the group's chat is open SHALL NOT take the
user away from it: the group's app bar SHALL offer the chat list, with the
number of unread DM messages.

#### Scenario: Two chats
- **WHEN** Alice is in the group and has a DM with Bob, and Bob sends her "hi"
  in the DM
- **THEN** her chat list shows the group first and "Bobby" below it with an
  unread mark

#### Scenario: A DM arrives while the group is open
- **WHEN** Alice reads the group's chat and Bob opens a DM with her and writes
- **THEN** Alice stays in the group's chat, and its app bar shows a chats
  button with an unread count

### Requirement: Open a DM
The members list SHALL offer "Message" on every other member when the Pi lists
protocol version 2. It SHALL open the existing open DM with that member, or ask
the core to open one (direct-messages: Opening a DM) and show it at once with
"Waiting for Bobby's devices" until the peer's first device is added.

#### Scenario: First message to Bob
- **WHEN** Alice taps "Message" on Bob in the members list and has no DM with him
- **THEN** the app asks the core to open the DM and shows the empty DM, where
  Alice can type at once; her message is sent once Bob's phone is added

#### Scenario: Old Pi
- **WHEN** the Pi lists only protocol version 1
- **THEN** the members list shows no "Message" action

### Requirement: DM screen
A DM SHALL use the chat screen, titled with the peer's display name, without
admin actions and without `everyone` in the mention picker. A closed DM SHALL
show its history with a banner "Bobby was removed" and no composer.

#### Scenario: Picker in a DM
- **WHEN** Alice types `@` in her DM with Bob
- **THEN** the picker offers Bob only

#### Scenario: Closed DM
- **WHEN** the core marks Alice's DM with Bob closed
- **THEN** the DM shows its messages, the banner, and no composer
