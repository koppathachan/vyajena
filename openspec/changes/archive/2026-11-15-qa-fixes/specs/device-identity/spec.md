## MODIFIED Requirements

### Requirement: Device revocation
Any authenticated device of a user SHALL be able to revoke another device of the
same user with `Devices.RevokeDevice`. A revoked device SHALL fail authentication
from then on with `PERMISSION_DENIED`, and its open stream SHALL be closed with
`PERMISSION_DENIED`; the server enforces both.

#### Scenario: Lost phone
- **WHEN** a user revokes their lost phone from their tablet
- **THEN** the phone's next authentication fails with `PERMISSION_DENIED` and its
  open stream closes with `PERMISSION_DENIED`
