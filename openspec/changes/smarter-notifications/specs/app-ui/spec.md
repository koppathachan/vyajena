## ADDED Requirements

### Requirement: Show who messaged
The app SHALL offer a per-device setting "Show who messaged", default **off**, that
controls whether notifications name the sender. While it is off, notifications show
only the generic, type-aware text ("New message", "3 new in Work"); while it is on,
a DM or group notification may name the sender ("Message from Ali", "Ali in Work").
The setting SHALL affect only how this device renders its own local notifications
and SHALL NOT be sent to the server. The app enforces this.

#### Scenario: Default is private
- **WHEN** Bob installs the app and never changes the setting
- **THEN** his notifications name no one, and the setting reads off

#### Scenario: Turning it on
- **WHEN** Bob turns "Show who messaged" on and Alice sends him a DM
- **THEN** his next notification reads "Message from Ali"

### Requirement: Type-aware notification wording
The app SHALL word a local notification by the conversation type from the core's
summary (local-notifications: The device builds the notification locally): a DM, a
named group (named in the text), the lobby, or an admin-event (a fixed phrase). The
app SHALL NOT show any message body, caption or decrypted content in a notification.
The app enforces this.

#### Scenario: Group named, DM not
- **WHEN** Bob has unread messages in the group "Work" and in a DM, with "Show who
  messaged" off
- **THEN** the group notification reads "3 new in Work" and the DM notification "2
  new messages", neither showing any message text

### Requirement: A notification opens its conversation
The app SHALL, when the user taps a notification, open the conversation it is for
(by `group_id`), starting the core and catching up as on any open (app-ui:
Foreground and background), and SHALL clear that conversation's notification once it
is on screen. An admin-event notification SHALL open the relevant screen (the chat
list, or the affected conversation if it still exists). The app enforces this.

#### Scenario: Tap opens the group
- **WHEN** Bob taps "3 new in Work"
- **THEN** the app opens the "Work" chat, the core catches up, and the "Work"
  notification is cleared
