## ADDED Requirements

### Requirement: Finish sends in the background
When the app goes to the background the app SHALL ask the core whether any own
message is still unsent. If none, it SHALL stop the core. If some, it SHALL keep
the core running until every one is accepted or failed, or until one minute after
the connection succeeded, whichever comes first, and then SHALL stop it. The minute
SHALL start when the core connects, not when the app goes to the background. If
the core is not connected within one minute of going to the background, the app
SHALL stop it. Returning to the app SHALL cancel the wait and leave the core
running. On iOS the app SHALL ask the system for background time for this wait and
SHALL stop the core when the system says the time is over. The app enforces this.

#### Scenario: Message sends in the background
- **WHEN** Bob sends a message that still shows ⏳ and switches to another app, and
  the core is connected
- **THEN** the core keeps running, the server accepts the message, and the app
  stops the core at once

#### Scenario: One minute after connecting
- **WHEN** Bob goes to the background with a message unsent, the core connects
  after 20 seconds, and the message is still unsent a minute after that
- **THEN** the app stops the core, and the message waits for the next time the app
  opens

#### Scenario: No connection
- **WHEN** Bob goes to the background with a message unsent and the core is still
  not connected a minute later
- **THEN** the app stops the core

#### Scenario: Nothing to send
- **WHEN** Bob goes to the background and every message he sent was accepted
- **THEN** the app stops the core at once, as before

#### Scenario: Back to the app
- **WHEN** Bob returns to the app while the core is finishing a send
- **THEN** the wait ends and the core keeps running
