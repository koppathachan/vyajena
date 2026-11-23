## ADDED Requirements

### Requirement: Bottom navigation
The app SHALL present a bottom navigation bar with three destinations — Chats,
Contacts and Profile — where the Profile destination's icon is the user's own round
avatar (their profile photo, or a monogram when none), not a gear. Selecting Profile
SHALL open the user's profile, settings and server info. Switching tabs SHALL
preserve each tab's state and scroll, and SHALL NOT discard a half-typed message in
an open conversation. The app enforces this.

#### Scenario: Three tabs
- **WHEN** the app opens
- **THEN** it shows a bottom bar with Chats, Contacts and a Profile tab whose icon is
  the user's round avatar

#### Scenario: Avatar opens profile, not a gear
- **WHEN** the user taps the avatar tab
- **THEN** the app opens the profile/settings/server-info screen

### Requirement: Chats list
The Chats tab SHALL show one unified conversation list: the `lobby` system group
pinned first, then admin-created named groups, then DMs newest-first, with closed
DMs greyed at the bottom. Each row SHALL show an avatar, the conversation's name, a
last-message preview with a media glyph for an image, video or voice message, a
timestamp, and an unread count with the mention badge. A closed-DM row SHALL carry
the swipe-to-remove affordance defined in `delete-messages` (message-deletion:
Remove a closed DM). The app enforces this.

#### Scenario: Unified list order
- **WHEN** the user has the lobby, a "Weekend" named group and a DM with Bob
- **THEN** the Chats tab lists "lobby" first, then "Weekend", then "Bob", each with a
  preview, timestamp and unread count

#### Scenario: Closed DM at the bottom
- **WHEN** Alice's DM with the removed Bob is closed
- **THEN** it shows greyed at the bottom with a swipe-to-remove affordance

### Requirement: Conversation screen
The app SHALL use one conversation screen for the lobby, named groups and DMs:
message bubbles incoming-left and outgoing-right, the sender's name shown in groups
only and never in a DM, tappable links, and inline players for image, audio and
video. The composer SHALL offer an attachment sheet (photo, video, audio, file) and
a mic button, beside the text field. The app bar SHALL show back, the group or peer
avatar, a title and subtitle, and an overflow menu; admin actions SHALL appear only
on an admin device. The app enforces this.

#### Scenario: Sender name in a group but not a DM
- **WHEN** the same user reads a named group and a DM
- **THEN** other members' messages show their sender name in the named group, and no
  sender name in the DM

#### Scenario: Attachment sheet
- **WHEN** the user taps the attachment control
- **THEN** a sheet offers photo, video, audio and file, and a mic button records a
  voice message

### Requirement: Contacts tab
The Contacts tab SHALL list the server's members, each with an avatar and name,
showing an `admin` badge on the admin and a "you" marker on the current user. It
SHALL offer an Invite action and, on an admin device, entry points to create a group
and to replace a user. Tapping a member SHALL open the existing DM with them or start
one. The app enforces this.

#### Scenario: Member list markers
- **WHEN** Bob opens Contacts
- **THEN** the admin row shows an `admin` badge and Bob's own row shows "you"

#### Scenario: Tap to message
- **WHEN** Alice taps Bob in Contacts
- **THEN** the app opens her DM with Bob, creating it if none exists

#### Scenario: Admin-only entry points
- **WHEN** a non-admin opens Contacts
- **THEN** no create-group, replace-user or Invite action is shown

### Requirement: Profile tab
The Profile tab SHALL show the user's own profile (display name and about, editable),
the server info (server id and connection state), and settings, reached by the avatar
tab. The app enforces this.

#### Scenario: Edit own profile
- **WHEN** the user opens the Profile tab and changes their display name
- **THEN** the app updates the profile through the core as before

#### Scenario: Server info present
- **WHEN** the user opens the Profile tab
- **THEN** it shows the server id and the current connection state
