## MODIFIED Requirements

### Requirement: Removing a user
The admin SHALL remove a user by committing Removes of all that user's devices and
calling `Admin.RemoveUser`, which revokes every device of that user. A revoked
device SHALL receive no further envelopes. `Admin.RemoveUser` SHALL also revoke every
unused invite for that username. In the same removal transaction, before the revoke
closes each live stream, the server SHALL deliver to each of the removed user's own
**connected** devices an account-level removal notice (a new `ServerFrame` variant,
design D6) and a `DmClosed` for every DM that device was in, so an online removed
device learns of the removal and clears its DMs in-band; this delivery is best-effort,
because a revoked device cannot authenticate again, and the authoritative signal for a
device that was offline at removal is the `PERMISSION_DENIED` it receives on its next
`Authenticate` (client-runtime: Connection lifecycle). After removal the username
SHALL be free for a new invite, so the admin can reset a user or re-issue them under a
new username; the new signup brings a new identity key, which members see as a changed
identity key. The server enforces the revoke, the invite revocation and the notice
delivery.

#### Scenario: User removed
- **WHEN** the admin removes Bob
- **THEN** Bob's devices fail authentication and are no longer group members

#### Scenario: Removed device is told in-band
- **WHEN** the admin removes Bob while Bob's phone has a live `Connect` stream
- **THEN** the server delivers the account-level removal notice, and a `DmClosed` for
  each of Bob's DMs, on that stream before closing it with the revoke

#### Scenario: Removed device was offline
- **WHEN** Bob's phone was offline when the admin removed Bob and later reconnects
- **THEN** `Devices.Authenticate` answers `PERMISSION_DENIED` and the phone never
  receives the removal notice, which is acceptable because the core treats that
  `PERMISSION_DENIED` as `Removed`

#### Scenario: Admin resets a user
- **WHEN** the admin removes Bob and then creates an invite for `bob@chat`
- **THEN** the server issues the invite, and Bob's device, once it has reset to a
  clean no-account state (client-runtime: Device reset), registers as `bob@chat` with
  a new identity key

#### Scenario: Admin re-invites under a new username
- **WHEN** the admin removes `bob@chat` and creates an invite for `robert@chat`
- **THEN** the server issues the invite and a device with no account registers as
  `robert@chat`, a fresh user the admin then adds to the groups

#### Scenario: Old invite dies with the removal
- **WHEN** an unused invite for `bob@chat` exists and the admin removes Bob
- **THEN** registering with that old invite fails, and only an invite created after
  the removal works
