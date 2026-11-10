# key-directory Specification

## Purpose
TBD - created by archiving change core-wire-protocol. Update Purpose after archive.
## Requirements
### Requirement: KeyPackage upload
A device SHALL upload MLS KeyPackages with `Keys.UploadKeyPackages`: one-time
KeyPackages plus exactly one last-resort KeyPackage. The Pi SHALL accept a
KeyPackage only if it parses, uses ciphersuite
`MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`, its credential carries the
uploading device's certificate, its signature key equals that certificate's
MLS signature key, and its leaf capabilities list the admin group-context
extension type (`0xF0A1`), without which the device could not be added to the
group.

#### Scenario: Valid upload
- **WHEN** a device uploads 20 one-time KeyPackages and one last-resort KeyPackage
  that all pass validation
- **THEN** the Pi stores them for that device

#### Scenario: KeyPackage for another device
- **WHEN** a device uploads a KeyPackage whose credential names a different device
- **THEN** the Pi rejects the whole upload with `PERMISSION_DENIED`

#### Scenario: KeyPackage without the admin extension capability
- **WHEN** Bob's phone uploads a KeyPackage whose capabilities do not list
  extension type `0xF0A1`
- **THEN** the Pi rejects the whole upload with `INVALID_ARGUMENT` and stores
  nothing

### Requirement: KeyPackage fetch
`Keys.FetchKeyPackages` SHALL take a user id and an optional list of device ids
and return, for each non-revoked device of that user, or only for the listed
devices when the list is given, one KeyPackage together with the device
certificate and the user's identity public key. The Pi SHALL consume KeyPackages
only for the devices it returns. The Pi SHALL hand out each one-time KeyPackage at
most once and SHALL fall back to the last-resort KeyPackage when none remain. The
Pi SHALL reject a fetch for an unknown user with `NOT_FOUND`, and a fetch listing
a device that is not a non-revoked device of that user with `NOT_FOUND`,
consuming nothing.

#### Scenario: One-time KeyPackage is consumed
- **WHEN** two callers fetch KeyPackages for the same user
- **THEN** they receive different one-time KeyPackages for each device

#### Scenario: One-time KeyPackages exhausted
- **WHEN** a device has no one-time KeyPackages left
- **THEN** the Pi returns its last-resort KeyPackage

#### Scenario: Fetch for one device
- **WHEN** the admin's device re-adds Bob's phone and fetches with Bob's user id
  and only the phone's device id
- **THEN** the Pi returns and consumes a KeyPackage for the phone only, and Bob's
  tablet keeps all its one-time KeyPackages

#### Scenario: Unknown user
- **WHEN** a device fetches KeyPackages for a user id the Pi does not know
- **THEN** the Pi rejects the call with `NOT_FOUND`

### Requirement: Replenishment signal
The Pi SHALL notify a connected device on its `Connect` stream when its one-time
KeyPackage count drops below 5, and the device SHALL upload more.

#### Scenario: Low water mark
- **WHEN** a device's one-time KeyPackage count drops from 5 to 4
- **THEN** the Pi sends that device a low-KeyPackages notice

### Requirement: Clients verify the identity chain
A client SHALL verify every fetched KeyPackage's device certificate against the
returned user identity key, and SHALL pin each user's identity key on first sight.
If a later fetch returns a different identity key for a pinned user, the client
SHALL refuse to add that user's devices and SHALL warn the user.

#### Scenario: Identity key changed
- **WHEN** a fetch returns an identity key for `bob@pi` that differs from the one
  pinned on this client
- **THEN** the client does not add Bob's devices and surfaces a warning

