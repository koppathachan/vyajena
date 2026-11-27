## ADDED Requirements

### Requirement: The device builds the notification locally
The core SHALL compute the user-facing notification on the device, after the device
wakes and catches up and decrypts locally (message-delivery: Ordered delivery and
catch-up), and SHALL NOT rely on the push payload for any of it. For each
conversation with unread messages it SHALL produce a typed summary carrying the
conversation **type** (DM, named group, lobby, or admin-event) and an **unread
count**, and SHALL NOT carry a sender, message body or caption in v1 (metadata only).
The app SHALL render a local notification from this summary and SHALL NOT read
decrypted content for it. The notification content SHALL be the same on iOS and
Android. The client enforces this.

#### Scenario: One group, three unread
- **WHEN** Bob's phone wakes to a content-free push, catches up, and decrypts three
  new messages in the named group "Work"
- **THEN** the core emits a summary `{type: named_group, group: "Work", unread: 3}`
  and the app shows a local notification "3 new in Work", with no sender and no
  message text

#### Scenario: A DM
- **WHEN** Bob has two unread messages in a DM with Alice
- **THEN** the app shows "2 new messages" with no name and no text

#### Scenario: Nothing decrypted leaves the core for the notification
- **WHEN** the app builds any notification
- **THEN** the summary it reads from the core contains a type and a count only, and
  never a sender, message body, caption or other decrypted content

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

### Requirement: Respect the notification detail level
The app SHALL honor the per-device notification detail level (app-ui: Notification
detail level) when rendering a notification from the core summary: at **Basic** it
SHALL show only the generic "New message" and SHALL NOT show the count or type name,
even though the summary carries them; at **More** it SHALL show the per-conversation
count and type and no sender. The level SHALL change only local rendering, and SHALL
NOT change the server push or whether a wake is sent. The client enforces this.

#### Scenario: Basic ignores the summary's detail
- **WHEN** detail is Basic and the core summary says `{type: named_group, group:
  "Work", unread: 3}`
- **THEN** the app shows "New message", not "3 new in Work"

#### Scenario: More renders the summary
- **WHEN** detail is More and the same summary arrives
- **THEN** the app shows "3 new in Work", naming no sender
