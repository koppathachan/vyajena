## ADDED Requirements

### Requirement: The device builds the notification locally
The core SHALL compute the user-facing notification on the device, after the device
wakes and catches up and decrypts locally (message-delivery: Ordered delivery and
catch-up), and SHALL NOT rely on the push payload for any of it. For each
conversation with unread messages it SHALL produce a typed summary carrying the
conversation **type** (DM, named group, lobby, or admin-event), an **unread
count**, and — only when the "Show who messaged" setting is on (app-ui: Show who
messaged) — the **display name** of the latest unread message's sender. The summary
SHALL NOT carry any message body or caption. The app SHALL render a local
notification from this summary and SHALL NOT read decrypted content for it. The
client enforces this.

#### Scenario: One group, three unread
- **WHEN** Bob's phone wakes to a content-free push, catches up, and decrypts three
  new messages in the named group "Work"
- **THEN** the core emits a summary `{type: named_group, group: "Work", unread: 3}`
  and the app shows a local notification "3 new in Work", with no message text

#### Scenario: A DM
- **WHEN** Bob has two unread messages in a DM with Alice and "Show who messaged" is
  off
- **THEN** the app shows "2 new messages" with no name and no text

#### Scenario: Nothing decrypted leaves the core for the notification
- **WHEN** the app builds any notification
- **THEN** the summary it reads from the core contains a type, a count and at most a
  sender display name, and never a message body, caption or other decrypted content

### Requirement: Conversation type without decryption
The core SHALL derive a conversation's type for the notification from the wire
`Envelope.group_id` (messaging.proto) mapped to the device's local conversation
record, which already classifies each conversation as the lobby, a named group, or a
DM, without decrypting the message. The client enforces this.

#### Scenario: DM vs group distinguished before content
- **WHEN** an `APPLICATION` envelope arrives whose `group_id` is a DM
- **THEN** the core classifies the notification as a DM from the `group_id` alone,
  independent of whether the body has been decrypted yet

### Requirement: Unread count, not push count
The core SHALL base the notification's count on its own per-conversation unread
state (as shown in the chat list), recomputed on each catch-up, and SHALL NOT
accumulate a count from the number of pushes received. A wake push that is missed,
collapsed or duplicated SHALL NOT change the count the user sees. The client
enforces this.

#### Scenario: Missed push does not corrupt the count
- **WHEN** two of three wake pushes for a conversation are collapsed by the platform
  and the device catches up once
- **THEN** the notification still shows "3 new", matching the core's unread state,
  not "1 new"

### Requirement: Admin-event notifications are fixed, content-free phrases
The core SHALL raise a notification for an admin-event that changes the user's own
access — the user was removed from a conversation (`RemovedNotice`), a conversation
was closed (`GroupClosed`, `DmClosed`), or the user was added to a new named group —
using a fixed phrase, never text derived from any message. It SHALL NOT raise a
notification for routine membership churn (other users joining or leaving, device
links, renames, or inactivity re-adds). The conversation name used in the phrase is
the device's local name for the conversation. The client enforces this.

#### Scenario: You were removed
- **WHEN** the admin removes Bob from "Work" and Bob's device receives
  `RemovedNotice` for it
- **THEN** Bob's phone shows a local notification "You were removed from Work", with
  no message content

#### Scenario: Routine join is not pushed
- **WHEN** Carol joins "Work" and Bob's device learns of it
- **THEN** Bob's phone shows the in-chat membership notice but raises no notification

### Requirement: Sender name only when opted in
The core SHALL include a sender display name in a notification summary only while the
"Show who messaged" setting is on (app-ui: Show who messaged); while it is off the
summary SHALL omit the sender and the app SHALL show only the generic, type-aware
text. The client enforces this.

#### Scenario: Opt-in reveals the sender
- **WHEN** "Show who messaged" is on and Alice sends Bob a DM
- **THEN** Bob's notification reads "Message from Ali"

#### Scenario: Default hides the sender
- **WHEN** "Show who messaged" is off and Alice sends Bob a DM
- **THEN** Bob's notification reads "New message" or "1 new message", naming no one
