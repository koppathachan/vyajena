## ADDED Requirements

### Requirement: Notification detail level
The app SHALL offer a per-device setting "Notification detail" with two levels,
**Basic** and **More**, default **More**. At Basic the app SHALL show only the
generic content-free notification ("New message") with no count, type name or
sender. At More the app SHALL show the per-conversation count and type
(Type-aware notification wording), and the sender only when "Show who messaged" is
on. The setting SHALL change only how this device renders its own local
notifications — not the server push, not whether a wake is sent — and SHALL NOT be
sent to the server. The app enforces this.

#### Scenario: Basic shows only the generic text
- **WHEN** Bob sets "Notification detail" to Basic and three messages arrive in the
  group "Work"
- **THEN** his phone shows "New message", with no count, group name or sender

#### Scenario: More shows count and type
- **WHEN** "Notification detail" is More (the default) and three messages arrive in
  "Work"
- **THEN** his phone shows "3 new in Work"

#### Scenario: Basic overrides the sender setting
- **WHEN** "Notification detail" is Basic and "Show who messaged" is on
- **THEN** notifications still read "New message" and name no one

### Requirement: Show who messaged
The app SHALL offer a per-device setting "Show who messaged", default **off**, that
controls whether notifications name the sender, and SHALL apply it only while
"Notification detail" is More (Notification detail level). While it is off, or while
detail is Basic, notifications name no one; while it is on and detail is More, a DM
or group notification may name the sender ("Message from Ali", "Ali in Work"). The
setting SHALL affect only how this device renders its own local notifications and
SHALL NOT be sent to the server. The app enforces this.

#### Scenario: Default is private
- **WHEN** Bob installs the app and never changes the setting
- **THEN** his notifications name no one, and the setting reads off

#### Scenario: Turning it on
- **WHEN** "Notification detail" is More, Bob turns "Show who messaged" on, and Alice
  sends him a DM
- **THEN** his next notification reads "Message from Ali"

### Requirement: Type-aware notification wording
The app SHALL, while "Notification detail" is More (Notification detail level), word
a local notification by the conversation type from the core's summary
(local-notifications: The device builds the notification locally): a DM, a named
group (named in the text), the lobby, or an admin-event (a fixed phrase); at Basic it
SHALL show only "New message". The app SHALL NOT show any message body, caption or
decrypted content in a notification at either level. The app enforces this.

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
