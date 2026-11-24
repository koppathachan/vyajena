## MODIFIED Requirements

### Requirement: Admin screens
The app SHALL show admin screens only on the admin's devices: create the group on
first start, invite a user by username (showing the invite text to share), list and
revoke unused invites, remove a user, change the group name, and replace a user. The
"Replace user" flow SHALL, in one guided screen, remove an existing user
(group-membership: Removing a user) and then create a fresh invite for them (same or a
new username), showing the new invite text to share. Other devices SHALL NOT show
these screens; the server rejects their admin RPCs anyway.

#### Scenario: Admin invites Bob
- **WHEN** the admin enters `bob` and taps Invite
- **THEN** the core calls `Admin.CreateInvite` and the app shows the invite text with
  a share button

#### Scenario: Admin replaces a user
- **WHEN** the admin picks Bob in "Replace user", confirms, and chooses to re-invite
  him as `robert`
- **THEN** the app removes Bob and then calls `Admin.CreateInvite` for `robert`,
  showing the new invite text to share

#### Scenario: Bob has no admin screens
- **WHEN** Bob opens the menu
- **THEN** he sees no invite, remove, replace or group-settings entries

## ADDED Requirements

### Requirement: Removed-device screen
The app SHALL show a terminal "You were removed from this server" screen when the core
emits `Removed`. The screen SHALL keep existing history readable and SHALL offer a
single "Start over" action that, after a confirmation that this device's local history
will be erased, calls the core's `reset()` (client-runtime: Device reset) and returns
to the invite screen. Until the user confirms, the app SHALL NOT erase anything and
SHALL NOT reconnect.

#### Scenario: Removed then start over
- **WHEN** the core emits `Removed` and the user taps "Start over" and confirms the
  erase
- **THEN** the app calls `reset()`, shows the invite screen, and a fresh invite
  onboards the device as the new user

#### Scenario: Removed but history kept
- **WHEN** the core emits `Removed` and the user does not tap "Start over"
- **THEN** the app still shows past messages read-only and does not reconnect
