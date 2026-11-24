## ADDED Requirements

### Requirement: The lobby appears without a manual create step
The app SHALL create the system lobby automatically on the admin's first device,
with no "Create the group" button to tap, whenever the device's user is the admin
(device-identity: Admin awareness) and no lobby exists on it yet: it calls
`Admin.CreateGroup` (group-membership: Group creation) on its own and lands the
admin in the lobby. A short setting-up state MAY show while the call is in flight. The manual create path
stays only for **additional named groups** (`Admin.CreateNamedGroup`), which the
admin still creates deliberately. The UI goes through the core (app-ui: UI goes
through the core).

This affects **only the admin's first device**. A later member's device never
creates the lobby: it waits to be added (app-ui: Join from an invite, "Waiting to
be added"). A second admin device that is being linked may briefly have no lobby
before it syncs; if its automatic `Admin.CreateGroup` is rejected because the lobby
already exists (group-membership: Group creation, `ALREADY_EXISTS`), the app SHALL
treat that as benign and wait to be synced into the existing lobby rather than
showing an error.

#### Scenario: Admin lands straight in the lobby
- **WHEN** the admin registers from the bootstrap invite on a fresh server and
  opens the app for the first time
- **THEN** the app calls `Admin.CreateGroup` with no manual step and shows the
  lobby, ready to invite people

#### Scenario: A linked admin device does not create a second lobby
- **WHEN** the admin links a second device that briefly has no lobby and its
  automatic `Admin.CreateGroup` is answered with `ALREADY_EXISTS`
- **THEN** the app shows no error and waits to be synced into the existing lobby

#### Scenario: A member still waits to be added
- **WHEN** Bob (not the admin) registers from his invite
- **THEN** his app shows the waiting screen and never tries to create a lobby

#### Scenario: Named groups are still created deliberately
- **WHEN** the admin creates an additional named group
- **THEN** the app creates it only on the admin's explicit action with
  `Admin.CreateNamedGroup`, not automatically
