## MODIFIED Requirements

### Requirement: Conversation screen
The app SHALL use one conversation screen for the lobby, named groups and DMs:
message bubbles incoming-left and outgoing-right, the sender's name shown in groups
only and never in a DM, tappable links, and inline players for image, audio and
video. The composer SHALL offer an attachment sheet (photo, video, audio, file) and
a mic button, beside the text field. The app bar SHALL show back, the group or peer
avatar, a title and subtitle. The conversation's overflow menu SHALL carry only
conversation-scoped actions — mute or unmute notifications, and for the lobby the
greyed notes that its name is fixed and it cannot be closed — and SHALL NOT
duplicate the shell's tabs: it SHALL NOT offer Members, Profile, Link a new device,
or the admin Invite, Invites and Remove-a-user actions, which the Contacts and
Profile tabs cover. A named group SHALL keep its own group-management menu (the
members of that group, add or remove a member, rename, close). When a conversation
has no conversation-scoped action, the app SHALL show no overflow button. The app
enforces this.

#### Scenario: Sender name in a group but not a DM
- **WHEN** the same user reads a named group and a DM
- **THEN** other members' messages show their sender name in the named group, and no
  sender name in the DM

#### Scenario: Attachment sheet
- **WHEN** the user taps the attachment control
- **THEN** a sheet offers photo, video, audio and file, and a mic button records a
  voice message

#### Scenario: Menu carries no tab duplicates
- **WHEN** Alice opens the lobby or a DM and opens the conversation's overflow menu
- **THEN** it offers mute or unmute and shows no Members, Profile, Link a new device,
  Invite, Invites or Remove-a-user entry

### Requirement: Contacts tab
The Contacts tab SHALL list the server's members, each with an avatar and name,
showing an `admin` badge on the admin and a "you" marker on the current user. It
SHALL offer an Invite action and, on an admin device, entry points to create a group
and to replace a user. Tapping another member SHALL open that member's profile
(Member profile); tapping the current user's own row SHALL open the Profile tab. The
app enforces this.

#### Scenario: Member list markers
- **WHEN** Bob opens Contacts
- **THEN** the admin row shows an `admin` badge and Bob's own row shows "you"

#### Scenario: Tap opens the member's profile
- **WHEN** Alice taps Bob in Contacts
- **THEN** Bob's profile opens, with a Message action that opens or starts her DM
  with Bob

#### Scenario: Admin-only entry points
- **WHEN** a non-admin opens Contacts
- **THEN** no create-group, replace-user or Invite action is shown

## ADDED Requirements

### Requirement: Member profile
The app SHALL show a member profile for another member, reached by tapping them on
the Contacts tab, presenting the member's avatar, display name and device count. It
SHALL offer a Message action that opens or starts the DM with that member when the
server offers DMs, asking the core afresh, and otherwise shows a notice that direct
messages are not available yet; and it SHALL offer report and block/unblock actions
(app-ui: Report a message or a user; Block and unblock a user). The app enforces
this.

#### Scenario: Profile shows the member
- **WHEN** Alice opens Bob's profile from Contacts
- **THEN** it shows Bob's name, avatar and device count, and a Message action

#### Scenario: Message opens the DM
- **WHEN** Alice taps Message on Bob's profile and the server offers DMs
- **THEN** the app calls `openDm` and the DM with Bob opens

#### Scenario: Message when DMs are not offered
- **WHEN** Alice taps Message on Bob's profile while DMs are not offered
- **THEN** a snack bar says direct messages are not available yet and no DM opens

#### Scenario: Block from the profile
- **WHEN** Alice blocks Bob from his profile and confirms
- **THEN** the app calls `blockUser` and marks Bob blocked

#### Scenario: Report from the profile
- **WHEN** Alice reports Bob from his profile with a reason
- **THEN** the app calls `reportUser`
