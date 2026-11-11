## ADDED Requirements

### Requirement: Last-resort KeyPackage refresh
Each device SHALL replace its last-resort KeyPackage on the Pi when the current
one is older than 7 days, and SHALL upload more one-time KeyPackages whenever it
receives a low-KeyPackages notice.

#### Scenario: Weekly refresh
- **WHEN** Bob's last-resort KeyPackage is 7 days old
- **THEN** his core uploads a new last-resort KeyPackage and the Pi hands out the
  new one from then on

### Requirement: Identity key lookup
`Keys.GetIdentityKeys` SHALL take a list of user ids and return each user's
current identity public key, consuming no KeyPackages. The Pi SHALL reject the
call with `NOT_FOUND`, returning nothing, if any listed user id is unknown or
has no non-revoked device.

#### Scenario: Learning keys without a KeyPackage
- **WHEN** Carol joins from a Welcome and looks up the identity keys of Alice
  and Bob
- **THEN** the Pi returns both keys and Alice's and Bob's devices keep all their
  one-time KeyPackages

#### Scenario: Unknown user in the list
- **WHEN** a device looks up `alice@pi` and an unknown `zed@pi`
- **THEN** the Pi rejects the call with `NOT_FOUND`

## MODIFIED Requirements

### Requirement: Clients verify the identity chain
A client SHALL verify every fetched KeyPackage's device certificate against the
returned user identity key, and SHALL pin each user's identity key on first sight,
learning it from `Keys.GetIdentityKeys` or `Keys.FetchKeyPackages`. If a later
fetch returns a different identity key for a pinned user, the client SHALL refuse
to add that user's devices and SHALL warn the user. A member processing a commit
from the admin that adds a device whose certificate does not verify against the
pinned key SHALL look up the user's identity key; if the Pi returns a new key that
verifies the certificate, the member SHALL accept the commit, pin the new key and
warn the user that the key changed. The same change in a commit from any other
member SHALL be rejected. A device joining from a Welcome whose tree holds a leaf
that does not verify against that user's pinned key SHALL look up the user's
identity key the same way, whoever signed the Welcome; if the Pi's key verifies
the leaf, the device SHALL join, pin the new key and warn the user.

#### Scenario: Identity key changed
- **WHEN** a fetch returns an identity key for `bob@pi` that differs from the one
  pinned on this client
- **THEN** the client does not add Bob's devices and surfaces a warning

#### Scenario: Admin re-adds a reset user
- **WHEN** the admin resets Bob and commits an Add of Bob's new phone, which is
  certified by Bob's new identity key
- **THEN** every other member looks up Bob's key, accepts the commit, pins the new
  key and shows a key-changed warning, and no member stalls

#### Scenario: Non-admin adds a device with a changed key
- **WHEN** a commit not from the admin adds a device of Bob whose certificate
  needs a new identity key
- **THEN** members reject the commit

#### Scenario: Back from holiday after a reset
- **WHEN** Alice's laptop, with Bob's old key pinned, is re-added after Bob was
  reset and receives a Welcome whose tree holds Bob's new phone
- **THEN** it looks up Bob's key, joins the group, pins the new key and shows a
  key-changed warning
