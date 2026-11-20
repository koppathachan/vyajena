## MODIFIED Requirements

### Requirement: Chat list
The app SHALL open on the chat list on every device that is in the group,
whether or not the user has a DM, showing the main group first, then each DM by
the peer's display name, newest message first, each with an unread mark and the
number of unseen mentions of the user; closed DMs SHALL be shown last and greyed.
Tapping a chat SHALL open it over the list. A DM that arrives while a chat is open
SHALL NOT take the user away from it; the user sees its unread mark on returning
to the list.

#### Scenario: Opens on the chat list
- **WHEN** Alice's device is in the group and she opens the app, whether or not
  she has a DM
- **THEN** she lands on the chat list with the group first

#### Scenario: Two chats
- **WHEN** Alice is in the group and has a DM with Bob, and Bob sends her "hi"
  in the DM
- **THEN** her chat list shows the group first and "Bobby" below it with an
  unread mark

#### Scenario: A DM arrives while a chat is open
- **WHEN** Alice has the group's chat open over the list and Bob writes in a DM
  with her
- **THEN** Alice stays in the group's chat, and the DM shows an unread mark once
  she returns to the list

## ADDED Requirements

### Requirement: Create affordance
The chat list SHALL show a `+` floating action button to start a new
conversation. The button SHALL offer **New DM** on every device, but only when
the core's `dmsAvailable` is true: choosing it lets the user pick another group
member and opens the DM through the core's `openDm`, showing it at once (the same
path as the members list's "Message"). The button SHALL offer **New group** only
on the admin's devices, where the core's `isAdmin` is true: choosing it creates
the group through the core's `createGroup`. A non-admin device SHALL NOT offer New
group, and a device whose server does not offer DMs SHALL NOT offer New DM; when
neither action is available the button SHALL NOT be shown. The client enforces
this; the server rejects a non-admin's group RPCs anyway.

#### Scenario: Alice starts a DM with Bob
- **WHEN** Alice taps `+`, chooses New DM and picks Bob, with whom she has no DM
- **THEN** the app calls the core's `openDm` for Bob and shows the DM at once,
  where she can type; her first message goes out once Bob's phone is added

#### Scenario: New group only for the admin
- **WHEN** Bob, who is not the admin, opens the `+` menu
- **THEN** he sees New DM but no New group, while the admin sees both

#### Scenario: Admin creates the group
- **WHEN** the admin taps `+` and chooses New group
- **THEN** the app calls the core's `createGroup`

#### Scenario: No DM on an old server
- **WHEN** a non-admin's server lists only protocol version 1, so `dmsAvailable` is
  false
- **THEN** the chat list shows no `+` button
