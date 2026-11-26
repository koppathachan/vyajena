## ADDED Requirements

### Requirement: Wake push payload is content-free and metadata-minimal
The server SHALL build every wake push to FCM or APNs (message-delivery:
Content-free wake pings) so that its payload carries no message content, no sender,
no `group_id` and no message count. The payload MAY carry only the platform wake
mechanism and, as a fallback, the one fixed generic string "New message"; all
user-visible detail beyond that generic string SHALL be computed on the device
(local-notifications: The device builds the notification locally). On iOS the push
SHALL be an alert carrying the generic string with `mutable-content` set, so the
device can rewrite it locally while the generic string shows if it cannot. On
Android the push SHALL be a high-priority data message carrying no content, waking
the app to build the notification. The server enforces the payload shape; the same
rule governs both providers.

#### Scenario: iOS payload carries only a wake and the generic fallback
- **WHEN** the server sends a wake push to Bob's offline iPhone for a new message
- **THEN** the APNs payload's only text is "New message", it sets `mutable-content`,
  and it contains no sender, `group_id` or count

#### Scenario: Android payload carries no content
- **WHEN** the server sends a wake push to Bob's offline Android phone
- **THEN** the FCM payload is a data message with no message content, sender,
  `group_id` or count, and no notification body other than the generic fallback path

#### Scenario: No count in the payload even when many are queued
- **WHEN** five messages are queued for Bob's offline device
- **THEN** the wake push still carries no number; the device computes "5 new" itself
  after catching up

### Requirement: Content-free wake for an own-access admin-event
The server SHALL send one content-free wake push (Wake push payload is content-free
and metadata-minimal) to a device when the device's own user is removed from a
conversation or a conversation the user is in is closed, and when the user is added
to a new named group, even though the triggering envelope is control traffic
(message-delivery: Content-free wake pings as revised by `push-kind-aware`). The
server SHALL derive these from the membership it already holds authoritatively and
SHALL put none of it in the payload; the device renders the fixed phrase from the
core notice it receives on its next `Connect` (local-notifications: Admin-event
notifications are fixed, content-free phrases). The server enforces this.

#### Scenario: Removal wakes the removed device
- **WHEN** the admin removes Bob from "Work" and Bob's phone is offline with a push
  token
- **THEN** the server sends one content-free wake push, carrying nothing about the
  removal, and Bob's phone shows "You were removed from Work" after it catches up

#### Scenario: Ordinary control backlog still does not wake
- **WHEN** an offline device's only backlog is a commit that re-keys the group and
  does not change that user's membership
- **THEN** the server sends no wake push (unchanged from `push-kind-aware`)
