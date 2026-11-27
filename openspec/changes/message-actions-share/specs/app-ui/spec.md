## ADDED Requirements

### Requirement: Copy a message
The app SHALL offer **Copy** in a chat message's long-press sheet when the message
has text to copy: a text body, or an image with a non-empty caption. Copy
SHALL put that text on the device clipboard (`Clipboard.setData`) and confirm with
a "Copied" snackbar, writing the text nowhere else and sending nothing. The app
SHALL NOT offer Copy on a media-only message, a deleted-message tombstone or an
unsupported body. Copy SHALL behave the same in the lobby, named groups and DMs.
The app enforces this.

#### Scenario: Copy a text message
- **WHEN** the user long-presses a message whose body is the text "meet at 6" and
  taps Copy
- **THEN** the clipboard holds "meet at 6" and a "Copied" snackbar is shown

#### Scenario: Copy an image caption
- **WHEN** the user long-presses an image message captioned "on the roof" and taps
  Copy
- **THEN** the clipboard holds "on the roof"

#### Scenario: No Copy without text
- **WHEN** the user long-presses a media-only message, a tombstone or an
  unsupported body
- **THEN** the sheet shows no Copy entry

### Requirement: Forward a message
The app SHALL offer **Forward** in a text message's long-press sheet. Forward SHALL
open a picker of the user's other chats — the lobby, named groups and open DMs —
and SHALL exclude the chat the message is in and any closed group or DM. Choosing a
chat SHALL send the message's text into it through the normal send path
(`sendText` for the lobby, `sendMessageIn` otherwise) as a new standalone message,
carrying neither the original's mentions nor its reply quote, and SHALL confirm
with a snackbar naming the destination. The app SHALL NOT offer Forward on a
media-only message in this version (forwarding attachments is a separate change).
The app enforces this.

#### Scenario: Forward text to another chat
- **WHEN** the user long-presses a text message "hello" in the lobby, taps Forward
  and chooses the DM with Bob
- **THEN** the text "hello" is sent into Bob's DM as a new message and a snackbar
  names the destination

#### Scenario: The picker excludes the current and closed chats
- **WHEN** the forward picker opens for a message in the lobby while a DM is closed
- **THEN** the lobby is not listed and the closed DM is not listed

#### Scenario: Forward drops mentions and the reply quote
- **WHEN** the user forwards a message that mentions someone and replies to another
- **THEN** the sent message is plain text with no mention highlight and no reply
  quote

### Requirement: Vyajena is a share target
The app SHALL register as an operating-system share target for plain text and
images so other apps' share sheets list Vyajena. When the user shares such content
to Vyajena, the app SHALL open the chat picker and, on a chosen chat, compose the
content there: shared text SHALL be sent as a text message, and a shared image
SHALL be sent through the normal image send path as a new attachment. The share
entry point SHALL hand only the raw shared content to the app; all encryption and
sending SHALL happen in the app through the core, so no key, ciphertext or
decrypted message crosses the share boundary. If no user is registered on the
device yet, the app SHALL ignore the share with a brief notice rather than compose.
On Android the app enforces this; on iOS the share extension target is completed
with the owner's signing (its own bundle id, provisioning and App Group) before it
appears.

#### Scenario: Share text from another app
- **WHEN** another app shares the text "look at this" to Vyajena and the user picks
  the lobby
- **THEN** the lobby receives "look at this" as a new text message

#### Scenario: Share a photo from the gallery
- **WHEN** the gallery shares one photo to Vyajena and the user picks a DM
- **THEN** the DM receives the photo as a new image message through the normal
  image send path

#### Scenario: Shared content never carries crypto
- **WHEN** content is shared to Vyajena
- **THEN** the share entry point passes only the raw text or image to the app, and
  the app's own core call does the encryption and sending

#### Scenario: Share before onboarding
- **WHEN** content is shared to Vyajena but no user is registered on the device
- **THEN** the app shows a brief notice and does not open the picker or send
