## ADDED Requirements

### Requirement: Automatic add of new users
The server SHALL send a `JoinNotice` to the admin's connected devices for each
non-revoked device that has never been a group member and whose user has no
member device: once that device has uploaded its first KeyPackages after
registering, and again each time an admin device connects, until the device is a
member. The first admin device to receive it
SHALL, without user action, fetch a KeyPackage for that device only, verify its
certificate (pinning the user's identity key on first sight), and commit its Add
with the Welcome in the same `COMMIT` envelope. A changed identity key SHALL
follow key-directory: Clients verify the identity chain: the admin's device
warns and adds the device once the admin accepts the key. An admin device that
loses the race gets `STALE_EPOCH`, sees the device is already a member and does
nothing more. The server decides who gets the notice; the admin's device enforces
the identity checks.

#### Scenario: Bob joins while the admin is online
- **WHEN** Bob registers his phone from the admin's invite and the admin's phone
  is connected
- **THEN** the server sends the admin's phone a `JoinNotice` for Bob's phone, the
  admin's phone adds it with no one tapping anything, and Bob's phone receives
  the Welcome

#### Scenario: Admin offline when Bob joins
- **WHEN** Bob registers while no admin device is connected
- **THEN** Bob's app shows that it is waiting to be added, and the admin's phone
  adds Bob's phone as soon as it next connects

#### Scenario: Reset user joins again
- **WHEN** the admin resets Bob and Bob registers again with a new identity key
- **THEN** the admin's phone shows that Bob's key changed and adds his new phone
  once the admin accepts the key

#### Scenario: Two admin devices race
- **WHEN** the admin's phone and tablet both commit the Add of Bob's phone in the
  same epoch
- **THEN** the server accepts one, rejects the other with `STALE_EPOCH` together with
  its Welcome, and the losing device does nothing more
