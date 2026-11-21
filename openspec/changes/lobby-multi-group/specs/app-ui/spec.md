## MODIFIED Requirements

### Requirement: Chat list
The app SHALL show a chat list with the system lobby first, labelled "lobby",
then the admin's named groups by name and the DMs by the peer's display name,
newest message first, each with an unread mark and the number of unseen mentions
of the user; closed named groups and closed DMs SHALL be shown last and greyed.
Tapping a chat SHALL open it. When the user has at least one chat beyond the lobby
the app SHALL open on the chat list; with none it SHALL open on the lobby's chat.
A chat that gains a message while another chat is open SHALL NOT take the user
away from the open one: its app bar SHALL offer the chat list, with the number of
unread messages.

#### Scenario: Lobby, a group and a DM
- **WHEN** Alice is in the lobby, is in the "Weekend" named group, and has a DM
  with Bob
- **THEN** her chat list shows "lobby" first, then "Weekend" and "Bobby", each with
  its own unread mark

#### Scenario: A message arrives while a chat is open
- **WHEN** Alice reads the lobby and a message arrives in the "Weekend" group
- **THEN** Alice stays in the lobby, and its app bar shows a chats button with an
  unread count

### Requirement: Admin screens
The app SHALL show admin screens only on the admin's devices: create the lobby on
first start, invite a user by username (showing the invite text to share), list
and revoke unused invites, remove a user, create a named group by picking its
members, rename a named group, and close a named group. The app SHALL show the
create-group and close-group actions only when the server lists protocol version
3. Other devices SHALL not show these screens; the server rejects their admin RPCs
anyway.

#### Scenario: Admin invites Bob
- **WHEN** the admin enters `bob` and taps Invite
- **THEN** the core calls `Admin.CreateInvite` and the app shows the invite text
  with a share button

#### Scenario: Admin creates a named group
- **WHEN** the admin taps "Create group", picks Bob and Carol, and confirms
- **THEN** the core creates a named group and adds Bob's and Carol's devices, and
  the group appears in the chat list

#### Scenario: Admin closes a group
- **WHEN** the admin opens the "Weekend" group and taps "Close group" and confirms
- **THEN** the core calls `Admin.CloseGroup` and the group becomes read-only for
  everyone

#### Scenario: Bob has no admin screens
- **WHEN** Bob opens the menu
- **THEN** he sees no invite, remove, create-group or close-group entries

## ADDED Requirements

### Requirement: Named group screen
A named group SHALL use the chat screen, titled with the group's name, showing a
placeholder title "Unnamed group" until the admin sets a name. The admin's add-member,
remove-member and rename actions SHALL appear only on the admin's devices, and
`@everyone` in the mention picker SHALL mean everyone in that group. A closed named
group SHALL show its history with a banner "This group was closed" and no composer.

#### Scenario: Members see the group's name
- **WHEN** the admin has named the group "Weekend"
- **THEN** every member's screen is titled "Weekend"

#### Scenario: Only the admin moderates from the screen
- **WHEN** Bob, a non-admin member, opens the named group
- **THEN** he sees no add-member, remove-member or rename action

#### Scenario: Closed group
- **WHEN** the core marks the "Weekend" group closed
- **THEN** the screen shows its messages, the banner "This group was closed", and
  no composer
