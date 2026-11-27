## ADDED Requirements

### Requirement: Notification detail level
The app SHALL offer a per-device setting "Notification detail" with two levels,
**Basic** and **More**, default **Basic**. At Basic the app SHALL show only the
generic content-free notification ("New message") with no count or type name. At More
the app SHALL show the per-conversation count and type (Type-aware notification
wording) and no sender. The setting SHALL change only how this device renders its own
local notifications — not the server push, not whether a wake is sent — and SHALL NOT
be sent to the server. The app SHALL show the same notification content at a given
level on iOS and Android. The app enforces this.

#### Scenario: Basic is the default
- **WHEN** Bob installs the app, leaves "Notification detail" at its default, and
  three messages arrive in the group "Work"
- **THEN** his phone shows "New message", with no count or group name

#### Scenario: More shows count and type
- **WHEN** Bob switches "Notification detail" to More and three messages arrive in
  "Work"
- **THEN** his phone shows "3 new in Work", naming no sender

#### Scenario: Same on both platforms
- **WHEN** Bob has an iPhone and an Android phone, both at More, and the same two DMs
  and one "Work" message are unread
- **THEN** both show the same count + type content (e.g. "2 new DMs, 1 new in Work"),
  neither naming a sender

### Requirement: Type-aware notification wording
The app SHALL, while "Notification detail" is More (Notification detail level), word
a local notification by the conversation type from the core's summary
(local-notifications: The device builds the notification locally): a DM, a named
group (named in the text), the lobby, or an admin-event (a fixed phrase); at Basic it
SHALL show only "New message". The app SHALL NOT show a sender, message body, caption
or any decrypted content in a notification at either level. The app enforces this.

#### Scenario: Group named, DM not
- **WHEN** Bob (at More) has unread messages in the group "Work" and in a DM
- **THEN** the group notification reads "3 new in Work" and the DM notification "2
  new messages", neither showing a sender or any message text

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
