## ADDED Requirements

### Requirement: Re-add after an unusable Welcome
A device that cannot use a Welcome SHALL NOT stay out of the group for good.
When the core refuses a Welcome (it cannot be staged, or a leaf fails the
identity checks), and when a non-admin device's core starts with an account
but no group, it SHALL call `Keys.RequestReadd` with the refused Welcome's `server_seq` (0 at
start), its error text cut to `MAX_READD_REASON_BYTES`, and fresh KeyPackages.
The Pi SHALL answer `READD_OUTCOME_ASKED` only for a non-revoked member device
in the tree that is not already inactive, has no Welcome queued after
`refused_welcome_seq`, and has asked fewer than `MAX_READD_REQUESTS_PER_DAY`
times in the last 24 hours; then, in one transaction, it SHALL replace all of
the device's KeyPackages with the new ones, mark the device inactive and back
(issuing its external Remove proposal), and send a `RejoinNotice` for it to the
devices that may re-add it, which re-add it as in Automatic re-add. Otherwise
the Pi SHALL change nothing and answer `READD_OUTCOME_NOT_A_MEMBER`,
`READD_OUTCOME_WELCOME_QUEUED`, `READD_OUTCOME_ALREADY_WAITING` or
`READD_OUTCOME_LIMIT_REACHED`. The Pi enforces who may ask and how often; the
re-adding device enforces the identity checks, so the Pi can still add no one.

#### Scenario: Bob's phone refuses its Welcome
- **WHEN** the admin's phone adds Bob's new phone and Bob's core cannot stage
  the Welcome
- **THEN** Bob's core calls `Keys.RequestReadd` with the Welcome's `server_seq`
  and the error, the Pi answers `READD_OUTCOME_ASKED` and sends the admin's
  phone a `RejoinNotice`, the admin's phone commits Remove + Add from one of
  Bob's new KeyPackages, and Bob's phone joins from the new Welcome

#### Scenario: Stuck from an older build
- **WHEN** Bob's phone refused its Welcome on a build without this
  requirement, and starts again on one with it
- **THEN** its core calls `Keys.RequestReadd` with `refused_welcome_seq` 0, and
  is re-added the same way

#### Scenario: Still waiting for the first add
- **WHEN** Bob's new phone starts before any admin device has added it
- **THEN** the Pi answers `READD_OUTCOME_NOT_A_MEMBER`, stores none of the
  KeyPackages in the request, and Bob's phone waits for its first add as before

#### Scenario: Welcome on its way
- **WHEN** Bob's phone asks at start while its Welcome is still queued
- **THEN** the Pi answers `READD_OUTCOME_WELCOME_QUEUED` and changes nothing

#### Scenario: Welcome refused again and again
- **WHEN** Bob's phone has asked `MAX_READD_REQUESTS_PER_DAY` times in the last
  24 hours and refuses another Welcome
- **THEN** the Pi answers `READD_OUTCOME_LIMIT_REACHED` and changes nothing,
  and Bob's phone asks again at its next start after the window

#### Scenario: The Pi cannot add anyone with it
- **WHEN** a dishonest Pi sends the admin's phone a `RejoinNotice` for a device
  whose KeyPackage does not carry a certificate signed by its user's pinned
  identity key
- **THEN** the admin's phone refuses to add it, as for every add
