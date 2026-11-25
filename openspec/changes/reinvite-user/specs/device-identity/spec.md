## ADDED Requirements

### Requirement: Re-inviting an existing user
The admin SHALL be able to re-onboard a user who lost their keys (an uninstall or data
wipe) under the **same** username in one action, authorized solely by a fresh,
single-use, admin-minted invite, with no prior `Admin.RemoveUser`. The server enforces
this. Specifically:

- `Admin.CreateInvite` SHALL accept a present, non-removed username and issue an
  ordinary single-use invite bound to it, instead of rejecting it with
  `ALREADY_EXISTS`. Minting such an invite means "re-invite this user". It SHALL remain
  admin-only (`device-identity: Invites`).
- `Devices.Register` SHALL treat a valid, unused, unexpired invite whose bound username
  is a **present, non-removed** user as a **re-key**: it SHALL replace that user's
  `identity_public_key` with the newly supplied key, keep the **same `user_id`**, revoke
  **every** prior device of that user, and record the new device — all in one
  transaction, with no prior `Admin.RemoveUser`. The certificate SHALL still be required
  to name `username@server_id` of the invite's user, and the invite SHALL be marked used.
- An existing username presented **without** a valid fresh invite SHALL NOT re-key: an
  absent, used, revoked or expired invite is rejected before the user row is examined
  (unchanged `PERMISSION_DENIED`), and `Devices.Link` (device addition) continues to
  require the user's existing pinned identity key, never changing it.
- The removed-user re-key path and the new-user path SHALL be unchanged.

#### Scenario: Admin re-invites a reinstalled user on the same name
- **WHEN** `bob@chat` is present and non-removed, and an admin device calls
  `Admin.CreateInvite` for `bob@chat`
- **THEN** the server returns a single-use invite bound to `bob@chat` instead of
  `ALREADY_EXISTS`

#### Scenario: Register with a re-invite re-keys the user
- **WHEN** Bob's reinstalled phone calls `Devices.Register` with that invite, a fresh
  identity key and a certificate naming `bob@chat`
- **THEN** the server updates `bob@chat`'s `identity_public_key` to the new key, keeps
  the user id `bob@chat`, revokes all of Bob's prior devices, records the new device and
  marks the invite used — without any prior `Admin.RemoveUser`

#### Scenario: Re-key kicks a surviving device
- **WHEN** `bob@chat` still has an active device when the re-invite register succeeds
- **THEN** that device's auth is revoked and its next `Devices.Authenticate` returns
  `PERMISSION_DENIED`

#### Scenario: Existing user without a valid invite cannot re-key
- **WHEN** a device attempts `Devices.Register` for present `bob@chat` with an invite
  that is absent, used, revoked or expired
- **THEN** the server rejects it with `PERMISSION_DENIED` and changes neither
  `bob@chat`'s identity key nor its devices

#### Scenario: Re-invite is single-use
- **WHEN** a re-invite for `bob@chat` has been used once to re-key
- **THEN** a second `Devices.Register` with the same invite is rejected and re-keys
  nothing
