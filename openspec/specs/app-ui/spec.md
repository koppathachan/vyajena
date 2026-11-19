# app-ui Specification

## Purpose
TBD - created by archiving change flutter-app. Update Purpose after archive.
## Requirements
### Requirement: UI goes through the core
The app SHALL reach the Pi only through the Rust core over `flutter_rust_bridge`;
Dart code SHALL open no network connection and SHALL hold no key. Decrypted text
SHALL reach Dart only to be shown, and the app SHALL never write it to a file, a
log or a crash report, or send it anywhere. The app enforces this; the core keeps
the only copy of messages in its SQLite store.

#### Scenario: No socket in Dart
- **WHEN** the app's Dart code under `app/lib/` is checked
- **THEN** it imports no `dart:io` socket or HTTP client and no network package,
  and every Pi call goes through a bridge function

#### Scenario: Nothing logged
- **WHEN** Alice receives "hi" and the app writes its debug log
- **THEN** the log names the kind of event but not "hi"

### Requirement: Join from an invite
The app SHALL let a new user join by pasting the invite text the admin shared,
SHALL show the username and server address the invite names before joining, and
SHALL call the core's `register` with it. A Pi that fails the invite's TLS pin
SHALL be shown as a server identity error, not as a network error.

#### Scenario: Bob joins
- **WHEN** Bob pastes the invite the admin made for `bob` and taps Join
- **THEN** he sees `bob@pi` on `127.0.0.1:8443` first, the core registers his
  phone with `Devices.Register`, and he lands in the chat

#### Scenario: Waiting to be added
- **WHEN** Bob has registered but the admin's device has not added him yet
- **THEN** his app says it is waiting for the admin's device, and shows the chat
  once the core emits `JoinedGroup`

#### Scenario: Wrong pin
- **WHEN** the Pi at the invite's address presents a different TLS key
- **THEN** the app shows a server identity error and does not join

#### Scenario: Bad invite text
- **WHEN** Bob pastes text that is not an invite
- **THEN** the app says the invite is not valid and calls nothing

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

### Requirement: Composer limit
The app SHALL refuse to send text whose encoded content is over the core's
`MAX_CONTENT_BYTES`, SHALL say so before sending, and SHALL keep the text in the
composer.

#### Scenario: Very long paste
- **WHEN** Alice pastes text longer than the limit and taps Send
- **THEN** nothing is sent, the composer keeps the text, and the app says it is
  too long

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
- **WHEN** the core emits `IdentityKeyChanged` for `bob@pi`
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

### Requirement: Admin screens
The app SHALL show admin screens only on the admin's devices: create the group on
first start, invite a user by username (showing the invite text to share), list
and revoke unused invites, remove a user, and change the group name. Other devices
SHALL not show them; the Pi rejects their admin RPCs anyway.

#### Scenario: Admin invites Bob
- **WHEN** the admin enters `bob` and taps Invite
- **THEN** the core calls `Admin.CreateInvite` and the app shows the invite text
  with a share button

#### Scenario: Bob has no admin screens
- **WHEN** Bob opens the menu
- **THEN** he sees no invite, remove or group-settings entries

### Requirement: Profile and members
The app SHALL let each user set their own display name and about text, with the
username shown read-only, and SHALL list the group's members by user with the
number of devices each has.

#### Scenario: Alice renames herself
- **WHEN** Alice sets her display name to "Ali"
- **THEN** the core sends a `Profile`, and everyone sees "Ali (alice)"

### Requirement: Link a device
The app SHALL link a new device by text: the new device shows its link request,
the user pastes it on the device that holds the identity key, which approves it
and shows the approval text, which the user pastes back on the new device.

#### Scenario: Alice links her tablet
- **WHEN** Alice copies her tablet's link request to her phone, approves it, and
  pastes the approval on the tablet
- **THEN** the tablet joins the group as Alice's second device

### Requirement: Light Material 3 look
The app SHALL use Material 3 with `ColorScheme.fromSeed`, one muted seed colour
and the `neutral` dynamic scheme variant, light and dark following the system,
with stock Material widgets and no custom fonts, images or theme packages.

#### Scenario: Dark mode
- **WHEN** Alice's phone is in dark mode
- **THEN** the app uses the dark scheme from the same seed

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

### Requirement: Data on the phone
The app SHALL keep the core's database in the app's private storage. On Android it
SHALL set `allowBackup="false"`; on iOS it SHALL exclude the database from iCloud
backup and protect it with `NSFileProtectionComplete`. The core keeps keys in the
Android Keystore or the iOS Keychain (client-runtime: Key storage).

#### Scenario: Android backup
- **WHEN** Bob's Android phone backs up to Google Drive
- **THEN** the app's database and files are not in the backup

### Requirement: Send an image
The composer SHALL let the user pick an image and send it with an optional
caption, passing the picked bytes and caption through the core
(UI goes through the core); the app SHALL NOT transcode, inspect or upload image
bytes itself. The caption field SHALL offer the same `@` mention picker as a text
message. While the blob uploads the app SHALL show the image as sending. The app
enforces going through the core.

#### Scenario: Pick and send
- **WHEN** Alice picks a photo, types the caption "@Bob look" and sends
- **THEN** the app hands the bytes and caption to the core and shows the image
  as sending until it is accepted

#### Scenario: Sent while offline
- **WHEN** Alice sends a picture while her phone has no connection
- **THEN** the picture appears in the chat as sending, like a text, and goes out
  when the phone is connected again

#### Scenario: App does no image work
- **WHEN** Alice sends an image
- **THEN** the app passes the raw bytes to the core and never encodes WebP,
  strips EXIF or uploads a blob itself

### Requirement: Image bubble and viewer
An image message SHALL show as a bubble at the image's aspect ratio
(`width`/`height`), first the ThumbHash blur (decoded by the core), then the full
image once downloaded; the caption, with its mentions highlighted, SHALL show
under it. The full image SHALL download on tap (no thumbnail, no auto-download),
and a tap on the loaded image SHALL open a full-screen viewer. When the core
reports the image expired the bubble SHALL show the blur with "Media expired —
resend" and SHALL download nothing. A message this device sent SHALL offer
"Resend picture" in its long-press menu, which sends the picture again as a new
message. A download that fails without being expired SHALL say so and let the
user tap to try again. The app enforces this.

#### Scenario: Blur then full image
- **WHEN** Bob receives an image
- **THEN** the bubble shows the ThumbHash blur at the right aspect ratio and the
  size to download, tapping it downloads and shows the full image, and tapping
  that opens the viewer

#### Scenario: Expired image
- **WHEN** Bob opens an image the Pi has deleted
- **THEN** the bubble shows the blur with "Media expired — resend"

#### Scenario: Alice resends
- **WHEN** Bob tells Alice her picture expired and she long-presses it and picks
  "Resend picture"
- **THEN** the core sends the picture again as a new message, and the chat shows
  both; a message from another user has no such entry

#### Scenario: Failed download can be retried
- **WHEN** a download fails because the phone is offline
- **THEN** the bubble says it could not load the picture, and a tap tries again

#### Scenario: Caption with a mention
- **WHEN** an image arrives captioned "@Bob look" mentioning Bob
- **THEN** the caption shows under the image with "@Bobby" highlighted on Bob's
  device
