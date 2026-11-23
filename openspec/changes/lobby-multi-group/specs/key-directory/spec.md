## MODIFIED Requirements

### Requirement: KeyPackage fetch
`Keys.FetchKeyPackages` SHALL take a user id, an optional list of device ids and
an optional `group_id`, and return, for each non-revoked device of that user, or
only for the listed devices when the list is given, one KeyPackage together with
the device certificate and the user's identity public key. When `group_id` names
a DM, the server SHALL return only KeyPackages whose capabilities list the DM
extension `0xF0A2`, leave out devices with neither a one-time nor a last-resort
KeyPackage that lists it, and SHALL reject the fetch with `PERMISSION_DENIED`
unless the caller's device is a member of that DM and `user_id` is one of its two
users. When `group_id` names a named group, the server SHALL return only
KeyPackages whose capabilities list the named-group extension `0xF0A3`, leave out
devices with none, and SHALL reject the fetch with `PERMISSION_DENIED` unless the
caller's device is a member of that group. The server SHALL consume KeyPackages
only for the devices it returns. The server SHALL hand out each one-time KeyPackage
at most once and SHALL fall back to the last-resort KeyPackage when none remain.
The server SHALL reject a fetch for an unknown user with `NOT_FOUND`, a fetch
listing a device that is not a non-revoked device of that user with `NOT_FOUND`,
and a fetch for an unknown `group_id` with `NOT_FOUND`, consuming nothing.

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

#### Scenario: Fetch for a named group skips old devices
- **WHEN** the admin fetches Carol's KeyPackages with a named group's `group_id`
  and Carol's tablet still runs a version 2 app
- **THEN** the server returns and consumes a KeyPackage only for Carol's devices
  that list `0xF0A3`

#### Scenario: Non-member fetches for a named group
- **WHEN** Bob's phone, not a member of the "Weekend" group, fetches with that
  group's `group_id`
- **THEN** the server rejects the call with `PERMISSION_DENIED` and consumes nothing
