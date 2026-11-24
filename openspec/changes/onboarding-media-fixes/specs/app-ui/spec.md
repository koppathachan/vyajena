## ADDED Requirements

### Requirement: A typed invite is accepted
The invite entry field SHALL accept an invite the user types, not only one pasted.
The field SHALL NOT auto-capitalize or auto-correct its text, and the core SHALL
match the `vyajena-invite:` prefix case-insensitively, so a leading capital the
keyboard inserts (e.g. `Vyajena-invite:`) still parses. The payload after the
prefix stays case-sensitive (it is base64url). The UI goes through the core (app-ui:
UI goes through the core).

#### Scenario: Keyboard capitalizes the prefix
- **WHEN** the user types an invite whose prefix the keyboard capitalized to
  `Vyajena-invite:` followed by a valid payload
- **THEN** the app parses it as a valid invite, the same as a pasted
  `vyajena-invite:` one

#### Scenario: The field does not auto-capitalize
- **WHEN** the invite field is focused on a fresh install
- **THEN** it does not auto-capitalize or auto-correct what the user types

### Requirement: Media the server will not store shows an error
The app SHALL show a clear failed state, with a resend option, on an image or voice
message the server will not store (its blob store is off or unavailable), rather
than an endless "sending" state (app-ui: Send an image; message-content: Audio
messages). The failure comes from
the core (client-runtime: Media the server will not store fails promptly).

#### Scenario: Server with no blob store
- **WHEN** the admin's server has no `[blob_store]` configured and Bob sends a
  voice message
- **THEN** within a short time the voice message shows a failed state with a reason,
  not a perpetual hourglass, and Bob can resend it
