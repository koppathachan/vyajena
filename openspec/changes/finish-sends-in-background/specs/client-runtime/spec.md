## ADDED Requirements

### Requirement: Unsent messages query
The core SHALL tell its caller whether any of this device's own messages is still
sending, meaning stored locally and not yet accepted or failed by the server. Messages
the server accepted or rejected SHALL NOT count. The core enforces this; the app
uses it to decide whether to keep the core running when it goes to the background.

#### Scenario: Message waiting to send
- **WHEN** Bob sends a message while his phone has no connection
- **THEN** the query says a message is unsent, and after the server accepts it
  the query says none is

#### Scenario: Nothing waiting
- **WHEN** every message Bob sent was accepted or failed
- **THEN** the query says none is unsent
