## MODIFIED Requirements

### Requirement: KeyPackage fetch
`Keys.FetchKeyPackages` SHALL take a user id, an optional list of device ids and
an optional `group_id`, and return, for each non-revoked device of that user, or
only for the listed devices when the list is given, one KeyPackage together with
the device certificate and the user's identity public key. When `group_id` names
a DM, the Pi SHALL return only KeyPackages whose capabilities list the DM
extension `0xF0A2` (a one-time one if the device has one left, else its
last-resort one if that lists it), leave out devices with neither, and SHALL reject the fetch with `PERMISSION_DENIED` unless the caller's device is
a member of that DM and `user_id` is one of its two users. The Pi SHALL consume
KeyPackages only for the devices it returns. The Pi SHALL hand out each one-time
KeyPackage at most once and SHALL fall back to the last-resort KeyPackage when
none remain. The Pi SHALL reject a fetch for an unknown user with `NOT_FOUND`,
and a fetch listing a device that is not a non-revoked device of that user with
`NOT_FOUND`, consuming nothing.

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

#### Scenario: Fetch for a DM skips old devices
- **WHEN** Alice's phone fetches Bob's KeyPackages with the Alice–Bob DM's
  `group_id`, and Bob's tablet still runs 1.0.0
- **THEN** the Pi returns and consumes a KeyPackage for Bob's phone only

#### Scenario: Carol fetches for someone else's DM
- **WHEN** Carol's phone fetches Bob's KeyPackages with the Alice–Bob DM's
  `group_id`
- **THEN** the Pi rejects the call with `PERMISSION_DENIED` and consumes nothing
