## MODIFIED Requirements

### Requirement: Bootstrap admin
The server SHALL create a single bootstrap invite for the configured admin user id and
print its invite payload to the Server log once whenever no **active** admin exists and
there is no usable bootstrap invite. An active admin is the admin user id present with
`removed_at IS NULL`; the gate is no longer "any users exist". The user created or
re-keyed from the bootstrap invite SHALL be the admin. On a start where the admin is
active, the server SHALL bootstrap nothing, whatever other users exist.

#### Scenario: First start
- **WHEN** the server starts with no users
- **THEN** it creates and prints one bootstrap invite for the admin user id

#### Scenario: Healthy server does not re-bootstrap
- **WHEN** the server restarts with the admin active (`removed_at IS NULL`) and other
  users present
- **THEN** it creates no bootstrap invite

#### Scenario: Re-bootstrap after an operator admin reset
- **WHEN** the server starts after `server admin-reset` marked the admin removed, with
  no usable bootstrap invite
- **THEN** it creates and prints one new bootstrap invite for the admin user id, and
  the admin re-registering with it takes the removed-user re-key path (admin-recovery:
  Recovered admin re-bootstraps with a fresh identity)
