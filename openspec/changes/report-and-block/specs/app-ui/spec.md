## ADDED Requirements

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
