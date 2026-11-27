# app-ui Specification

## Purpose
TBD - created by archiving change flutter-app. Update Purpose after archive.
## Requirements
### Requirement: UI goes through the core
The app SHALL reach the server only through the Rust core over `flutter_rust_bridge`;
Dart code SHALL open no network connection and SHALL hold no key. Decrypted text
SHALL reach Dart only to be shown, and the app SHALL never write it to a file, a
log or a crash report, or send it anywhere. The app enforces this; the core keeps
the only copy of messages in its SQLite store.

#### Scenario: No socket in Dart
- **WHEN** the app's Dart code under `app/lib/` is checked
- **THEN** it imports no `dart:io` socket or HTTP client and no network package,
  and every server call goes through a bridge function

#### Scenario: Nothing logged
- **WHEN** Alice receives "hi" and the app writes its debug log
- **THEN** the log names the kind of event but not "hi"

### Requirement: Join from an invite
The app SHALL let a new user join by pasting the invite text the admin shared,
SHALL show the username and server address the invite names before joining, and
SHALL call the core's `register` with it. A server that fails the invite's TLS pin
SHALL be shown as a server identity error, not as a network error.

#### Scenario: Bob joins
- **WHEN** Bob pastes the invite the admin made for `bob` and taps Join
- **THEN** he sees `bob@chat` on `127.0.0.1:8443` first, the core registers his
  phone with `Devices.Register`, and he lands in the chat

#### Scenario: Waiting to be added
- **WHEN** Bob has registered but the admin's device has not added him yet
- **THEN** his app says it is waiting for the admin's device, and shows the chat
  once the core emits `JoinedGroup`

#### Scenario: Wrong pin
- **WHEN** the server at the invite's address presents a different TLS key
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

### Requirement: Admin screens
The app SHALL show admin screens only on the admin's devices: create the group on
first start, invite a user by username (showing the invite text to share), list
and revoke unused invites, remove a user, and change the group name. Other devices
SHALL not show them; the server rejects their admin RPCs anyway.

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
SHALL show whether the core is connected. The connection indicator SHALL look
different in each state so that connecting never reads as offline: connected is
static in an active colour, connecting is animated in an active colour, offline is
static in a muted colour, and removed is unchanged. While the indicator is
connecting or offline and this device was not removed, tapping it SHALL call the
core's `reconnect_now`, and pulling the chat list down SHALL call `reconnect_now`
too; while connected or removed, tapping the indicator SHALL do nothing. The
client enforces this.

#### Scenario: Back to the app
- **WHEN** Bob switches back to the app after an hour in another app
- **THEN** the core reconnects, catches up in order, and the connection indicator
  turns connected

#### Scenario: Network back
- **WHEN** Bob turns airplane mode off while the core is waiting to retry
- **THEN** the app calls `reconnect_now` and the core reconnects without waiting
  out its backoff

#### Scenario: Connecting does not look offline
- **WHEN** the core is connecting
- **THEN** the indicator animates in the active colour, distinct from the muted
  static look it shows when offline

#### Scenario: Tap to retry
- **WHEN** Bob taps the connection indicator while it shows offline
- **THEN** the app calls `reconnect_now` so the core reconnects without waiting
  out its backoff

#### Scenario: Pull to refresh
- **WHEN** Bob pulls the chat list down
- **THEN** the app calls `reconnect_now`

#### Scenario: Nothing to retry
- **WHEN** Bob taps the indicator while it shows connected, or while this device
  was removed from the group
- **THEN** the app calls nothing

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
- **WHEN** Bob opens an image the server has deleted
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

### Requirement: Open a DM
The members list SHALL offer "Message" on every other member when the server lists
protocol version 2. It SHALL open the existing open DM with that member, or ask
the core to open one (direct-messages: Opening a DM) and show it at once with
"Waiting for Bobby's devices" until the peer's first device is added.

#### Scenario: First message to Bob
- **WHEN** Alice taps "Message" on Bob in the members list and has no DM with him
- **THEN** the app asks the core to open the DM and shows the empty DM, where
  Alice can type at once; her message is sent once Bob's phone is added

#### Scenario: Old server
- **WHEN** the server lists only protocol version 1
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

### Requirement: Mention picker
The composer SHALL show, when the user types `@` at the start of the text or
after a space, the group's other members by display name and username,
and "everyone", filtered by what follows the `@`. Picking one SHALL insert
"@" + the display name (or username) and record a mention of that user id
(or `everyone`); editing inside an inserted name SHALL drop that mention.
The client enforces this.

#### Scenario: Alice mentions Bob
- **WHEN** Alice types "hi @bo" and picks "Bobby (bob)"
- **THEN** the composer shows "hi @Bobby " and sending calls the core with a
  mention of `bob@chat` over "@Bobby"

#### Scenario: Everyone
- **WHEN** Carol types "@" and picks "everyone"
- **THEN** the composer shows "@everyone " and sends a mention of `everyone`

#### Scenario: Name edited away
- **WHEN** Alice inserts "@Bobby" and then deletes its last letter
- **THEN** the message is sent with no mention

### Requirement: Mentions of me
The app SHALL draw each mention as "@" + this device's name for the target
(or "@everyone"), highlight mentions of this device's user and `@everyone`
more strongly than other mentions, and show in the chat's app bar an `@`
badge with the number of unseen mentions. A mention SHALL become seen when
its message has been on screen while the app is in the foreground, or when
the user taps the badge, which jumps to the oldest unseen one. The push text
SHALL stay "New message". The client enforces this.

#### Scenario: Bob is mentioned
- **WHEN** Bob's phone shows Alice's "hi @Bob"
- **THEN** "@Bobby" is highlighted in the bubble

#### Scenario: Badge
- **WHEN** Bob has scrolled up to older messages and two messages mention him
- **THEN** the app bar shows an `@` badge with 2, and tapping it jumps to the
  older one and lowers the count to 1

#### Scenario: Not mentioned
- **WHEN** Carol's phone shows Alice's "hi @Bob"
- **THEN** "@Bobby" is drawn as a mention but not highlighted as hers, and
  Carol's badge does not change

### Requirement: Reply to a message
The app SHALL let the user reply to a message by swiping it to the right or
by long-pressing it and choosing Reply; the composer SHALL then show the
quote with a way to cancel. A reply SHALL show the replied-to user's name
and the quote above its text, and tapping the quote SHALL scroll to the
original when this device has it, else say that the original is not on this
device. The client enforces this.

#### Scenario: Swipe to reply
- **WHEN** Bob swipes Alice's "dinner at 8?" to the right and sends "yes"
- **THEN** the core gets a reply to that message, and the chat shows "yes"
  under "Alice (alice)" and "dinner at 8?"

#### Scenario: Long-press to reply
- **WHEN** Bob long-presses a message and taps Reply
- **THEN** the composer shows its quote and a cancel button

#### Scenario: Jump to the original
- **WHEN** Bob taps the quote of a reply whose original is in the chat
- **THEN** the chat scrolls to the original

#### Scenario: Original not on this device
- **WHEN** Bob's tablet taps the quote of a reply whose original it never had
- **THEN** the app says the original is not on this device

### Requirement: Read mark

The core SHALL expose a chat's read mark through `chat_read_mark(chat_id)`: the
local id of the newest message the user had read in that chat, as
`mark_chat_read` last set it, or 0 when the chat was never read. The accessor
SHALL only read; it SHALL NOT advance the mark. It SHALL work for the main
group and for DMs alike.

#### Scenario: Never read

- **WHEN** Bob has never read the main group's chat
- **THEN** `chat_read_mark(0)` returns 0

#### Scenario: Marked up to the newest read message

- **WHEN** Bob receives Alice's "one", calls `mark_chat_read(0)`, and then
  receives Alice's "two" without reading again
- **THEN** `chat_read_mark(0)` returns the local id of "one", and the chat's
  `unread` count is 1

#### Scenario: Advances only on a read

- **WHEN** Bob then calls `mark_chat_read(0)` again
- **THEN** `chat_read_mark(0)` returns the local id of "two"

### Requirement: Unread divider

The chat screen SHALL read the chat's read mark once when it opens, before it
advances the mark, and SHALL draw a single "New messages" divider immediately
above the first received message (not one of this user's own devices') whose
local id is greater than that snapshot. The divider SHALL stay fixed while the
chat is open, even as new messages arrive and the stored mark advances, and
SHALL be re-placed from a fresh snapshot the next time the chat opens. On open
the chat SHALL scroll to that first unread message instead of the bottom. When
no received message is newer than the snapshot the chat SHALL show no divider
and land at the bottom as before. When the first unread message is the oldest
message shown, with no message above it, the chat SHALL show no divider but
SHALL still land on that message. The behaviour SHALL be the same for the main
group and for DMs.

#### Scenario: New messages since last read

- **WHEN** Bob read the group's chat up to "one" last time, and "two" and
  "three" arrived from Alice while he was away, and he opens the chat
- **THEN** a "New messages" divider is shown below "one" and above "two", and
  the chat opens scrolled to "two"

#### Scenario: Nothing new

- **WHEN** Bob has read every message in the chat and opens it
- **THEN** no "New messages" divider is shown and the chat is at the bottom

#### Scenario: Divider stays put while reading

- **WHEN** the divider is shown above "two" and a new message "four" arrives
  from Alice while Bob is in the chat
- **THEN** the divider stays above "two" and does not move to "four"

#### Scenario: All unread

- **WHEN** Bob opens a chat he has never read
- **THEN** no divider is shown and the chat lands on the oldest message in the
  window

### Requirement: Scroll to bottom

The chat screen SHALL show a scroll-to-bottom button only while the chat is
scrolled up away from the newest message, and tapping it SHALL return the chat
to the newest message and hide the button. When the chat is already at the
newest message the button SHALL NOT be shown.

#### Scenario: Scrolled up

- **WHEN** Bob scrolls up from the newest message in a long chat
- **THEN** a scroll-to-bottom button appears

#### Scenario: Back to the bottom

- **WHEN** Bob taps the scroll-to-bottom button
- **THEN** the chat returns to the newest message and the button disappears

### Requirement: Record a voice message
The composer SHALL record a voice message by hold-to-record: the user presses and
holds the mic, releases to stop, and slides to cancel while holding. On release the
app SHALL show a preview with play, delete and send, and SHALL pass the recorded
audio to the core, which encodes and sends it (audio-messages: Recording and
encoding); the app SHALL NOT encode, transcode or upload audio itself. While the
blob uploads the app SHALL show the clip as sending. The app enforces going through
the core.

#### Scenario: Hold, release, send
- **WHEN** Alice holds the mic, speaks, releases, and taps send on the preview
- **THEN** the app hands the recorded audio to the core and shows the clip as
  sending until it is accepted

#### Scenario: Slide to cancel
- **WHEN** Alice holds the mic and slides to cancel before releasing
- **THEN** nothing is recorded into the chat and nothing is sent

#### Scenario: App does no audio work
- **WHEN** Alice sends a voice message
- **THEN** the app passes the recorded audio to the core and never encodes Opus or
  uploads a blob itself

### Requirement: Play a voice message
A voice message SHALL show as a bubble with a play/pause control, the waveform from
the body and the duration. The clip SHALL download on first tap (no auto-download),
then play; starting another clip SHALL stop the one playing. When the core reports
the audio expired the bubble SHALL show "Audio expired" and download nothing. A
message this device sent SHALL offer resend in its long-press menu, which sends the
clip again as a new message. The app enforces this.

#### Scenario: Download then play
- **WHEN** Bob taps a voice message for the first time
- **THEN** the core downloads and decrypts the clip and it plays, with the waveform
  filling as it plays

#### Scenario: One at a time
- **WHEN** a clip is playing and Bob taps another
- **THEN** the first stops and the second plays

#### Scenario: Expired audio
- **WHEN** Bob opens a voice message the server has deleted
- **THEN** the bubble shows "Audio expired" and downloads nothing

### Requirement: Links in messages
The app SHALL draw `http://` and `https://` URLs in message text and in image
captions as tappable links, and SHALL NOT linkify anything else (no bare domains,
`mailto:` or `tel:`). Tapping a link SHALL open a confirm sheet showing the full
destination URL with Open, Copy and Cancel, and only Open SHALL hand the URL to
the OS browser (`url_launcher`, external). The app SHALL fetch no link preview and
SHALL open no connection itself. Where a detected link range overlaps a mention,
the mention SHALL be drawn and the link range dropped. Links SHALL behave the same
in the lobby, named groups and DMs. The app enforces this.

#### Scenario: Tap a link
- **WHEN** Alice's message says "see https://example.com/x" and Bob taps the URL
- **THEN** a sheet shows "https://example.com/x" with Open, Copy and Cancel, and
  Open launches the external browser

#### Scenario: Only http/https
- **WHEN** a message says "visit example.com or mail a@b.com"
- **THEN** neither "example.com" nor "a@b.com" is tappable

#### Scenario: Link in a caption
- **WHEN** an image is captioned "https://example.com/photo"
- **THEN** the caption shows the URL as a tappable link under the image

#### Scenario: Link overlapping a mention
- **WHEN** a message's text has a mention span over the same characters as a
  detected URL
- **THEN** the app draws the mention and does not draw a second link over it

#### Scenario: Nothing is fetched
- **WHEN** a message contains a URL
- **THEN** the app shows no title, image or other preview for it and opens no
  connection until the user taps Open

### Requirement: Remove a closed DM from the list
The chat list SHALL offer a Remove affordance on a closed-DM row (swipe or
long-press) and, on confirm, SHALL ask the core to remove that DM locally
(message-deletion: Remove a closed DM), after which the row disappears from the
list. The affordance SHALL appear only on closed DMs, never on an open DM, the lobby
or a named group. The app enforces this.

#### Scenario: Remove a closed DM
- **WHEN** Alice swipes the closed "Bob" row in her chat list and confirms
- **THEN** the core removes the DM and the row disappears from her list on this and
  her other devices

#### Scenario: No Remove on an open chat
- **WHEN** Alice swipes the lobby row or an open DM row
- **THEN** no Remove affordance is offered

### Requirement: Mute a conversation from the chat list
The app SHALL offer a mute/unmute affordance for a conversation — on the
conversation's overflow menu and on a chat-row long-press — and, on use, SHALL ask
the core to set the mute preference (notification-prefs: Mute a conversation). A
muted row SHALL show a mute marker. The affordance SHALL be offered for a DM, the
lobby and a named group alike. The app enforces this.

#### Scenario: Mute from the chat row
- **WHEN** Alice long-presses the "Work" row and taps Mute
- **THEN** the core records the mute, the row shows a mute marker, and "Work" stops
  notifying on this and her other devices

#### Scenario: Unmute from the overflow menu
- **WHEN** Alice opens the muted "Work" conversation's overflow menu and taps
  Unmute
- **THEN** the mute marker disappears and notifications for "Work" resume

### Requirement: Mute the whole app
The Profile tab SHALL offer a single "mute all notifications" control that calls
the core to set the global mute flag (notification-prefs: Mute the whole app), and
SHALL show whether it is on. While it is on, the app SHALL make clear that
per-conversation settings are overridden. The app enforces this.

#### Scenario: Turn on mute all
- **WHEN** Alice turns on "mute all notifications" in the Profile tab
- **THEN** the core sets the global flag and the app shows every conversation as
  silenced until she turns it off

#### Scenario: Mute-all state is visible on muted rows
- **WHEN** "mute all" is on
- **THEN** the chat list conveys that notifications are globally off, not that each
  row was individually muted

### Requirement: Block and unblock a user
The app SHALL offer a Block action on a user — from a message's long-press menu and
from the user's profile — and an Unblock action for a blocked user, each calling the
core (moderation: Block a user). A blocked user SHALL be marked as blocked wherever
they appear, and the app SHALL warn, before blocking, that blocking hides that user's
messages and DMs on this device only. The app enforces this.

#### Scenario: Block from a message
- **WHEN** Alice long-presses one of Mallory's messages and taps Block
- **THEN** the app confirms that Mallory's content will be hidden on this device, and
  on confirmation the core blocks Mallory and her messages disappear from the view

#### Scenario: Unblock from the profile
- **WHEN** Alice opens the blocked Mallory's profile and taps Unblock
- **THEN** the core unblocks Mallory and her messages and any DM reappear

### Requirement: Report a message or a user
The app SHALL offer a Report action on a message — from the same long-press menu —
and on a user from their profile, each asking for a short reason and then calling the
core to send the report to the admin (moderation: Report a message; Report a DM
message; Report a user). After sending, the app SHALL confirm the report was sent and
SHALL NOT reveal the admin's identity beyond what the user already knows. The app
enforces this.

#### Scenario: Report a group message
- **WHEN** Alice long-presses Mallory's group message, taps Report, picks a reason and
  confirms
- **THEN** the app calls the core to send the report to the admin and shows that the
  report was sent

#### Scenario: Report a DM message
- **WHEN** Alice reports a message in her DM with Mallory
- **THEN** the app sends the report with the quoted content (moderation: Report a DM
  message) and confirms it was sent

#### Scenario: Offer both block and report together
- **WHEN** Alice opens the long-press menu on an abusive message
- **THEN** both Report and Block are offered, and she may do either or both

### Requirement: The admin sees reports
The admin's app SHALL present an incoming report distinctly from an ordinary DM
message, showing the reported user, the reason, and either the referenced message
(group/lobby) or the quoted content (DM), so the admin can review it and, if
warranted, remove the user through the existing admin screens (moderation: The admin
acts on a report with the existing removal; app-ui: Admin screens). The app enforces
the presentation.

#### Scenario: Admin reads a report
- **WHEN** a report DM reaches the admin's app
- **THEN** it is shown as a report naming the reported user and reason, with the
  referenced or quoted message, and a path to the existing remove-user action

### Requirement: Safety and moderation entry
The Profile tab SHALL include a "Safety & moderation" entry that shows the published
moderation policy (moderation: Published moderation policy): how to report and block,
that the admin reviews and acts within 24 hours, and the safety contact email, shown
as the `<SAFETY_CONTACT_EMAIL>` placeholder token until the owner's real address
replaces it before publishing. The app enforces this.

#### Scenario: Open the policy
- **WHEN** Alice opens "Safety & moderation" in the Profile tab
- **THEN** she sees how to report and block, the 24-hour review commitment, and the
  contact email (the `<SAFETY_CONTACT_EMAIL>` token until the owner's real address
  replaces it)

### Requirement: Deleting your account
The app SHALL offer a "Delete account" action in the Profile tab that lets a user
delete their own account and data from within the app (Apple Guideline 5.1.1(v);
Google Play account-deletion policy). Tapping it SHALL open an explicit confirmation
that names what is deleted — the account, the user's groups and DMs on this device,
and the server-held key packages, push tokens, queued messages and uploaded media —
and states that the deletion is **irreversible and immediate**, with no deactivation,
no cancellable grace period and no undo. Because the action is irreversible, the
confirmation SHALL require the user to **type their own username** to confirm: the
confirm control SHALL stay disabled until the typed text matches the user's username,
so no single tap or swipe can delete the account. Only that explicit typed
confirmation SHALL proceed to call the core's `delete_account()`. On success the app SHALL return to the
first-run / onboarding screen after the device wipe (account-deletion: The initiating
device wipes itself on success); on failure it SHALL keep the account and show the
error. Against a server too old to implement `Devices.DeleteAccount` the call fails
and the app shows that error rather than deleting (no new capability flag is added to
the wire; design D7). The client enforces the confirmation and the on-success
navigation.

#### Scenario: Delete account from Profile
- **WHEN** Bob opens Profile, taps "Delete account", reads the confirmation, types his
  username `bob` and confirms
- **THEN** the app calls `delete_account()`, and on an OK response wipes the device and
  returns to the onboarding screen

#### Scenario: Type-to-confirm gates the delete
- **WHEN** Bob taps "Delete account" but has not typed his username, or typed something
  that does not match it
- **THEN** the confirm control stays disabled and the app calls nothing

#### Scenario: Confirmation is required
- **WHEN** Bob taps "Delete account" but dismisses the confirmation without confirming
- **THEN** the app calls nothing and Bob's account is unchanged

#### Scenario: Failure keeps the account
- **WHEN** Bob confirms deletion but the core returns an error
- **THEN** the app keeps Bob signed in to his account and shows the error, without
  wiping local state

### Requirement: The admin sees no self-delete
The app SHALL NOT offer a working "Delete account" action to the admin's device
(account-deletion: The admin cannot self-delete through the RPC). The Profile tab on
the admin's device SHALL show the "Delete account" entry **greyed-out** with a short,
honest note — *"The admin account can't be deleted from the app. To reset it, run
`server admin-reset` on the server host (see admin-recovery)."* — so the admin cannot
lock themselves out from inside the app. The client enforces this using its admin
awareness (device-identity: Admin awareness).

#### Scenario: Admin's Profile has no delete
- **WHEN** the admin opens Profile
- **THEN** the "Delete account" entry is disabled (greyed-out) with the note that the
  admin account can't be deleted from the app and is reset by running `server
  admin-reset` on the host, and tapping it does nothing

#### Scenario: Member's Profile has delete
- **WHEN** an ordinary member opens Profile
- **THEN** the "Delete account" action is present and enabled

### Requirement: Out-of-sync re-sync affordance
The app SHALL show a clear in-chat banner stating the chat is out of sync, with a
**Re-sync** action that asks to be re-added, when the core signals that a chat is
out of sync — the device is wedged behind the group's epoch and cannot self-recover
(group-membership: Orphaned device self-recovery) — rather than leaving the
composer's send on an indefinite silent hourglass with no explanation. The app SHALL
NOT present this as the message having failed (its text is kept). The app enforces
this.

#### Scenario: Out-of-sync banner with a re-sync action
- **WHEN** the core emits the out-of-sync signal for the lobby because Bob's phone
  is wedged and could not be re-added
- **THEN** the lobby chat shows an "out of sync — re-sync" banner with a Re-sync
  button, and Bob's held message is shown as waiting to re-sync, not as a silent
  hourglass and not as failed

#### Scenario: Re-sync asks to be re-added
- **WHEN** Bob taps Re-sync
- **THEN** the app calls the core to request re-add, and once a device able to
  re-add him is online the banner clears and his held message is sent

### Requirement: Admin recovers a locked-out user
The admin's app SHALL expose, as a first-class and reachable flow, the recovery for
a user whose device is wiped or cannot be re-synced: remove the user
(`Admin.RemoveUser`), then create a fresh invite for the same username
(`Admin.CreateInvite`), which the user pastes to re-register. The app SHALL make
the remove and invite actions reachable to the admin (not only buried), SHALL state
that remove must precede re-invite (the server refuses an invite for a username
whose user still exists and is not removed), and SHALL note what is lost (the old
device's key and local history, and that the user's existing DMs were closed by the
removal). The app enforces reachability; the server enforces the remove-before-invite
rule.

#### Scenario: Admin re-invites a wiped user
- **WHEN** `thalasserykalaapam`'s phone was wiped and the admin opens the recovery
  flow
- **THEN** the admin finds Remove a user, removes `thalasserykalaapam`, then finds
  Invite a user and creates a fresh invite for the same username, and the app shows
  the invite text to share

#### Scenario: Invite before remove is refused with a clear reason
- **WHEN** the admin tries to create an invite for `thalasserykalaapam` before
  removing them
- **THEN** the server refuses it because the user still exists, and the app tells the
  admin to remove the user first

### Requirement: Invite expiry is surfaced
The app SHALL surface an invite's expiry wherever an invite is shown or used: when
the admin creates or displays an invite it SHALL state when the invite expires (an
invite is valid for `invite_ttl`, 7 days by default), and when a user pastes an
invite that the server rejects as expired the app SHALL say the invite has expired
and that a fresh one is needed, rather than showing a generic failure. This keeps the
recovery flow usable — a wiped user who does not paste the invite in time is told
plainly, and the admin can create another. The app enforces the surfacing; the server
enforces the TTL and reports expiry.

#### Scenario: A created invite shows when it expires
- **WHEN** the admin creates a fresh invite for `thalasserykalaapam`
- **THEN** the app shows the invite text together with when it expires (for example
  "expires in 7 days"), not the invite text alone

#### Scenario: Pasting an expired invite is explained
- **WHEN** `thalasserykalaapam` pastes an invite after it has expired and the server
  rejects it
- **THEN** the app says the invite has expired and a new one is needed, rather than a
  generic "could not join" error

### Requirement: Add a member to an existing named group
The admin's app SHALL provide a way to add an existing user to an existing named
group, not only when the group is created, using the existing add-to-group flow
(the lobby and named groups: admin devices commit the Add). Without it, a user who
is removed and re-invited (or any user added after a named group was created) cannot
be restored to that named group, because today the Members screen is view-only and
named-group membership can only be chosen at creation. The lobby and DMs re-add
automatically and are unaffected; this requirement covers named groups only. The app
enforces reachability; the server and clients enforce commit authority as today.

#### Scenario: Re-add a recovered user to a named group
- **WHEN** `thalasserykalaapam` has re-registered and is back in the lobby, and was
  previously in the named group "Weekend"
- **THEN** the admin opens "Weekend" and uses an add-a-member action to add
  `thalasserykalaapam` to it, rather than having to recreate the group

#### Scenario: Add a later user to an existing named group
- **WHEN** the admin created "Weekend" before Dave joined and now wants Dave in it
- **THEN** the admin can add Dave to "Weekend" from the group, with no need to
  recreate it

### Requirement: Lobby row styling
The chat list SHALL render the lobby row (`ChatKindInfo_Lobby`) distinctly from
all other rows: a `#` hash tile on an accent-tinted square in place of the
plain avatar, the lobby name in italics, a small "system" tag pill beside the
name, the row pinned to the top of the list, and a subtle accent-tinted row
background. This treatment SHALL apply only to the lobby, never to an
admin-created named group (`ChatKindInfo_Group`) or a DM (`ChatKindInfo_Dm`),
whose rows keep their plain avatar and upright name. The app enforces this.

#### Scenario: Lobby looks like the system space
- **WHEN** Alice opens the chat list with the lobby, a "Weekend" named group and a
  DM with Bob
- **THEN** the lobby row (`ChatKindInfo_Lobby`) is first, with a `#` hash tile on
  an accent-tinted square, its name in italics, a "system" pill beside it and a
  subtle accent row tint

#### Scenario: Only the lobby is styled
- **WHEN** the same list is shown
- **THEN** the "Weekend" group row and Bob's DM row show no hash tile, no italics,
  no "system" pill and no accent tint

#### Scenario: Lobby stays pinned first
- **WHEN** a DM or named group has the newest message
- **THEN** the lobby row is still first in the list

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

