## MODIFIED Requirements

### Requirement: Respect the notification detail level
The device SHALL always render the metadata notification: one line per conversation
with unread messages giving its per-conversation count and type, plus the fixed
admin-event phrases — never a sender and never any message text. There is no
per-device detail level to choose or respect; the device never shows a generic
"New message"-only notification when there is something to show.

#### Scenario: Always the count and type
- **WHEN** the device wakes, catches up, and three messages are unread in "Work"
- **THEN** it posts "3 new in Work", naming no sender — never a generic
  content-free line in its place

#### Scenario: Nothing to show posts nothing
- **WHEN** the device wakes and nothing is unread and no admin-event is pending
- **THEN** it posts no notification
