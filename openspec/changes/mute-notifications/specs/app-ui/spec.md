## ADDED Requirements

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
