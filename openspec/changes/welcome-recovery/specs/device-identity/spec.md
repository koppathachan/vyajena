## ADDED Requirements

### Requirement: Fresh keys for a new signup
A device with no account in its local store SHALL create a new device auth
key and MLS signature key (and, at `Devices.Register`, a new user identity
key) for each signup or link request, replacing any key left in the OS key
store by an earlier install, and SHALL delete a leftover user identity key
when it starts a link request. The client enforces this: the iOS Keychain
keeps items after the app is deleted, so without it a reinstalled phone
would sign up again with a revoked device id.

#### Scenario: Bob reinstalls after the admin removed him
- **WHEN** the admin removes Bob, Bob deletes and reinstalls the app on his
  iPhone (the Keychain still holds his old keys) and pastes a new invite
- **THEN** his core registers with a new device id and identity key,
  `Devices.Register` succeeds, and the admin's phone adds him

#### Scenario: Linking after a reinstall
- **WHEN** a phone that once held Bob's identity key is reinstalled and starts
  a link request for Alice
- **THEN** the request carries new device keys and the phone no longer holds
  an identity key
