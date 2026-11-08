## ADDED Requirements

### Requirement: KeyPackage upload
A device SHALL upload MLS KeyPackages with `Keys.UploadKeyPackages`: one-time
KeyPackages plus exactly one last-resort KeyPackage. The Pi SHALL accept a
KeyPackage only if it parses, uses ciphersuite
`MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`, its credential carries the
uploading device's certificate, and its signature key equals that certificate's
MLS signature key.

#### Scenario: Valid upload
- **WHEN** a device uploads 20 one-time KeyPackages and one last-resort KeyPackage
  that all pass validation
- **THEN** the Pi stores them for that device

#### Scenario: KeyPackage for another device
- **WHEN** a device uploads a KeyPackage whose credential names a different device
- **THEN** the Pi rejects the whole upload with `PERMISSION_DENIED`

### Requirement: KeyPackage fetch
`Keys.FetchKeyPackages` SHALL take a user id and return, for each non-revoked
device of that user, one KeyPackage together with the device certificate and the
user's identity public key. The Pi SHALL hand out each one-time KeyPackage at most
once and SHALL fall back to the last-resort KeyPackage when none remain.

#### Scenario: One-time KeyPackage is consumed
- **WHEN** two callers fetch KeyPackages for the same user
- **THEN** they receive different one-time KeyPackages for each device

#### Scenario: One-time KeyPackages exhausted
- **WHEN** a device has no one-time KeyPackages left
- **THEN** the Pi returns its last-resort KeyPackage

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
