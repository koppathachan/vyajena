## ADDED Requirements

### Requirement: Reusable review admin invite
A server whose config sets `[review] reusable_admin_invite = true` SHALL hold
exactly one reusable invite for the configured admin user id. It SHALL never
expire and SHALL never be marked used. Each `Devices.Register` with it SHALL take
over the admin seat in one transaction: revoke every active device of the admin,
revoke the admin's other unused invites, re-seed the system lobby exactly as
`server admin-reset` does (admin-recovery: Break-glass re-seeds the admin's lobby),
install the new identity key for the admin (clearing `removed_at`) and register the
new device. The newest registration wins; the revoked devices' open `Connect`
streams SHALL be closed at once. Ordinary invites stay single-use and expiring. A
server without the flag SHALL hold no reusable invite and SHALL treat a reusable
invite row as unknown. The server enforces this.

#### Scenario: Reviewer joins as the admin
- **WHEN** the review server runs with `reusable_admin_invite = true` and a reviewer
  pastes its reusable invite and calls `Devices.Register` for `review@review.example`
- **THEN** the reviewer's device is the admin device, `Admin.CreateGroup` seeds the
  lobby at epoch 0, and the invite is still usable

#### Scenario: Second reviewer takes over
- **WHEN** Alice holds the admin seat from the reusable invite and Bob registers with
  the same invite and a new identity key
- **THEN** Alice's device is revoked and her `Connect` stream closes, Bob's key is the
  admin's identity key, and Bob's `Admin.CreateGroup` seeds a fresh lobby

#### Scenario: Same reviewer re-joins after a reinstall
- **WHEN** Alice reinstalls the app and registers again with the reusable invite
- **THEN** she is the admin again on her new device and her old device stays revoked

#### Scenario: Members are re-added after a takeover
- **WHEN** Carol was a lobby member invited by Alice and Bob takes over the admin seat
- **THEN** Carol is announced as waiting, Bob's device re-adds her with a fresh
  Welcome, and her client shows the admin re-trust prompt (admin-recovery: Admin
  re-key forces peer re-trust)

#### Scenario: Ordinary invites stay single-use
- **WHEN** the admin on the review server mints an invite with `Admin.CreateInvite`
  and Carol registers with it
- **THEN** a second `Devices.Register` with that code fails with `PERMISSION_DENIED`

#### Scenario: Flag off, no reusable invite
- **WHEN** a server starts without a `[review]` table, or with
  `reusable_admin_invite = false`, and a device calls `Devices.Register` with a code
  that a reusable invite row holds
- **THEN** the server mints no reusable invite, rejects the code with
  `PERMISSION_DENIED`, and changes nothing

## MODIFIED Requirements

### Requirement: Bootstrap admin
The server SHALL create a single bootstrap invite for the configured admin user id and
print its invite payload to the Server log once whenever no **active** admin exists and
there is no usable bootstrap invite. An active admin is the admin user id present with
`removed_at IS NULL`; the gate is no longer "any users exist". The user created or
re-keyed from the bootstrap invite SHALL be the admin. On a start where the admin is
active, the server SHALL bootstrap nothing, whatever other users exist. A server with
`[review] reusable_admin_invite = true` SHALL create no bootstrap invite: its reusable
invite (device-identity: Reusable review admin invite) is the admin's way in.

#### Scenario: First start
- **WHEN** the server starts with no users
- **THEN** it creates and prints one bootstrap invite for the admin user id

#### Scenario: Healthy server does not re-bootstrap
- **WHEN** the server restarts with the admin active (`removed_at IS NULL`) and other
  users present
- **THEN** it creates no bootstrap invite

#### Scenario: Re-bootstrap after an operator admin reset
- **WHEN** the server starts after `server admin-reset` marked the admin removed, with
  no usable bootstrap invite
- **THEN** it creates and prints one new bootstrap invite for the admin user id, and
  the admin re-registering with it takes the removed-user re-key path (admin-recovery:
  Recovered admin re-bootstraps with a fresh identity)

#### Scenario: Review server mints no bootstrap invite
- **WHEN** a server with `reusable_admin_invite = true` starts with no users
- **THEN** it creates no bootstrap invite and creates its reusable admin invite instead
