## ADDED Requirements

### Requirement: Remove a closed DM from the list
The chat list SHALL offer a Remove affordance on a closed-DM row (swipe or
long-press) and, on confirm, SHALL ask the core to remove that DM locally
(message-deletion: Remove a closed DM), after which the row disappears from the
list. The affordance SHALL appear only on closed DMs, never on an open DM, the lobby
or a named group. The app enforces this.

#### Scenario: Remove a closed DM
- **WHEN** Alice swipes the closed "Bob" row in her chat list and confirms
- **THEN** the core removes the DM and the row disappears from her list on this and
  her other devices

#### Scenario: No Remove on an open chat
- **WHEN** Alice swipes the lobby row or an open DM row
- **THEN** no Remove affordance is offered
