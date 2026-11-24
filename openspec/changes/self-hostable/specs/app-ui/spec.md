## ADDED Requirements

### Requirement: Published build is operator-neutral in the UI
The app's onboarding copy SHALL name no single implied server or group: it SHALL
refer to "the server you were invited to" rather than assume one owner-run group.
There SHALL be no pre-invite "choose a server" step (design D2, option A2): the
invite the user pastes is the only thing that names the server, so onboarding goes
straight to the invite. The app's store identity (bundle id, display name) is the
publisher's and is out of scope for this requirement; only server identity is
runtime-chosen, and it comes from the invite.

#### Scenario: Onboarding text carries no server host
- **WHEN** the onboarding screen is rendered on a fresh install
- **THEN** no server host, `server_id`, or group name is baked into its strings,
  and the only server shown is the one the pasted invite names

#### Scenario: No choose-server step precedes the invite
- **WHEN** Bob opens the app for the first time
- **THEN** the app shows the invite step directly, with no screen asking him to
  type a server address before the invite
