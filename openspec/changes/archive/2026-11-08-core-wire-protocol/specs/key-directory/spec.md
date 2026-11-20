## ADDED Requirements

### Requirement: KeyPackage upload
A device SHALL upload MLS KeyPackages with `Keys.UploadKeyPackages`: one-time
KeyPackages plus exactly one last-resort KeyPackage. The server SHALL accept a
KeyPackage only if it parses, uses ciphersuite
`MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`, its credential carries the
uploading device's certificate, and its signature key equals that certificate's
MLS signature key.

#### Scenario: Valid upload
- **WHEN** a device uploads 20 one-time KeyPackages and one last-resort KeyPackage
  that all pass validation
- **THEN** the server stores them for that device

#### Scenario: KeyPackage for another device
- **WHEN** a device uploads a KeyPackage whose credential names a different device
- **THEN** the server rejects the whole upload with `PERMISSION_DENIED`

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
returned user identity key, and SHALL pin each user's identity key on first sight.
If a later fetch returns a different identity key for a pinned user, the client
SHALL refuse to add that user's devices and SHALL warn the user.

#### Scenario: Identity key changed
- **WHEN** a fetch returns an identity key for `bob@chat` that differs from the one
  pinned on this client
- **THEN** the client does not add Bob's devices and surfaces a warning
