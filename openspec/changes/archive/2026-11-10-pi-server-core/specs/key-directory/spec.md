## MODIFIED Requirements

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
