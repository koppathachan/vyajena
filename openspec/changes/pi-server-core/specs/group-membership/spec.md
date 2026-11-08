## MODIFIED Requirements

### Requirement: Removing a user
The admin SHALL remove a user by committing Removes of all that user's devices and
calling `Admin.RemoveUser`, which revokes every device of that user. A revoked
device SHALL receive no further envelopes. `Admin.RemoveUser` SHALL also revoke
every unused invite for that username. After removal the username SHALL be free
for a new invite, so the admin can reset a user who lost their identity
device; the new signup brings a new identity key, which members see as a changed
identity key.

#### Scenario: User removed
- **WHEN** the admin removes Bob
- **THEN** Bob's devices fail authentication and are no longer group members

#### Scenario: Admin resets a user
- **WHEN** the admin removes Bob and then creates an invite for `bob@pi`
- **THEN** the Pi issues the invite, and Bob's new phone registers as `bob@pi`
  with a new identity key

#### Scenario: Old invite dies with the removal
- **WHEN** an unused invite for `bob@pi` exists and the admin removes Bob
- **THEN** registering with that old invite fails, and only an invite created
  after the removal works
