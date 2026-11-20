# key-directory Specification

## Purpose
TBD - created by archiving change core-wire-protocol. Update Purpose after archive.
## Requirements
### Requirement: KeyPackage upload
A device SHALL upload MLS KeyPackages with `Keys.UploadKeyPackages`: one-time
KeyPackages plus exactly one last-resort KeyPackage. The server SHALL accept a
KeyPackage only if it parses, uses ciphersuite
`MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`, its credential carries the
uploading device's certificate, its signature key equals that certificate's
MLS signature key, and its leaf capabilities list the admin group-context
extension type (`0xF0A1`), without which the device could not be added to the
group.

#### Scenario: Valid upload
- **WHEN** a device uploads 20 one-time KeyPackages and one last-resort KeyPackage
  that all pass validation
- **THEN** the server stores them for that device

#### Scenario: KeyPackage for another device
- **WHEN** a device uploads a KeyPackage whose credential names a different device
- **THEN** the server rejects the whole upload with `PERMISSION_DENIED`

#### Scenario: KeyPackage without the admin extension capability
- **WHEN** Bob's phone uploads a KeyPackage whose capabilities do not list
  extension type `0xF0A1`
- **THEN** the server rejects the whole upload with `INVALID_ARGUMENT` and stores
  nothing

### Requirement: KeyPackage fetch
`Keys.FetchKeyPackages` SHALL take a user id and an optional list of device ids
and return, for each non-revoked device of that user, or only for the listed
devices when the list is given, one KeyPackage together with the device
certificate and the user's identity public key. The server SHALL consume KeyPackages
only for the devices it returns. The server SHALL hand out each one-time KeyPackage at
most once and SHALL fall back to the last-resort KeyPackage when none remain. The
Server SHALL reject a fetch for an unknown user with `NOT_FOUND`, and a fetch listing
a device that is not a non-revoked device of that user with `NOT_FOUND`,
consuming nothing.

#### Scenario: One-time KeyPackage is consumed
- **WHEN** two callers fetch KeyPackages for the same user
- **THEN** they receive different one-time KeyPackages for each device

#### Scenario: One-time KeyPackages exhausted
- **WHEN** a device has no one-time KeyPackages left
- **THEN** the server returns its last-resort KeyPackage

#### Scenario: Fetch for one device
- **WHEN** the admin's device re-adds Bob's phone and fetches with Bob's user id
  and only the phone's device id
- **THEN** the server returns and consumes a KeyPackage for the phone only, and Bob's
  tablet keeps all its one-time KeyPackages

#### Scenario: Unknown user
- **WHEN** a device fetches KeyPackages for a user id the server does not know
- **THEN** the server rejects the call with `NOT_FOUND`

### Requirement: Replenishment signal
The server SHALL notify a connected device on its `Connect` stream when its one-time
KeyPackage count drops below 5, and the device SHALL upload more.

#### Scenario: Low water mark
- **WHEN** a device's one-time KeyPackage count drops from 5 to 4
- **THEN** the server sends that device a low-KeyPackages notice

### Requirement: Clients verify the identity chain
A client SHALL verify every fetched KeyPackage's device certificate against the
returned user identity key, and SHALL pin each user's identity key on first sight,
learning it from `Keys.GetIdentityKeys` or `Keys.FetchKeyPackages`. If a later
fetch returns a different identity key for a pinned user, the client SHALL refuse
to add that user's devices and SHALL warn the user. A member processing a commit
from the admin that adds a device whose certificate does not verify against the
pinned key SHALL look up the user's identity key; if the server returns a new key that
verifies the certificate, the member SHALL accept the commit, pin the new key and
warn the user that the key changed. The same change in a commit from any other
member SHALL be rejected. A device joining from a Welcome whose tree holds a leaf
that does not verify against that user's pinned key SHALL look up the user's
identity key the same way, whoever signed the Welcome; if the server's key verifies
the leaf, the device SHALL join, pin the new key and warn the user.

#### Scenario: Identity key changed
- **WHEN** a fetch returns an identity key for `bob@chat` that differs from the one
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

### Requirement: Last-resort KeyPackage refresh
Each device SHALL replace its last-resort KeyPackage on the server when the current
one is older than 7 days, and SHALL upload more one-time KeyPackages whenever it
receives a low-KeyPackages notice.

#### Scenario: Weekly refresh
- **WHEN** Bob's last-resort KeyPackage is 7 days old
- **THEN** his core uploads a new last-resort KeyPackage and the server hands out the
  new one from then on

### Requirement: Identity key lookup
`Keys.GetIdentityKeys` SHALL take a list of user ids and return each user's
current identity public key, consuming no KeyPackages. The server SHALL reject the
call with `NOT_FOUND`, returning nothing, if any listed user id is unknown or
has no non-revoked device.

#### Scenario: Learning keys without a KeyPackage
- **WHEN** Carol joins from a Welcome and looks up the identity keys of Alice
  and Bob
- **THEN** the server returns both keys and Alice's and Bob's devices keep all their
  one-time KeyPackages

#### Scenario: Unknown user in the list
- **WHEN** a device looks up `alice@chat` and an unknown `zed@chat`
- **THEN** the server rejects the call with `NOT_FOUND`

