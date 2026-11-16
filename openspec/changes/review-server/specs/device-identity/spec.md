## MODIFIED Requirements

### Requirement: Invites
Only the admin's devices SHALL be able to create invites. An invite SHALL be
single-use, SHALL name the user id it creates, SHALL expire after 7 days unless
used, and SHALL be shared as an invite payload containing the relay address, the
Pi's TLS key pin and the invite code. The admin SHALL be able to list and revoke
unused invites. The one exception is the review server's bootstrap invite in
review mode (review-server: Review mode), which never expires and stays usable
after use; a used one is not listed and cannot be revoked.

#### Scenario: Admin creates an invite
- **WHEN** an admin device calls `Admin.CreateInvite` for `alice@pi`
- **THEN** the Pi returns a single-use code bound to `alice@pi`

#### Scenario: Non-admin creates an invite
- **WHEN** a non-admin device calls `Admin.CreateInvite`
- **THEN** the Pi rejects it with `PERMISSION_DENIED`

#### Scenario: Review bootstrap invite
- **WHEN** the review server in review mode creates its bootstrap invite
- **THEN** it never expires, and every signup with it starts a fresh review

### Requirement: Signup with an invite
A user's first device SHALL register by calling `Devices.Register` with an invite
code, the new user identity public key and a device certificate for itself. The
Pi SHALL create the user and the device atomically and mark the invite used. In
review mode a used bootstrap invite SHALL still be accepted and SHALL start a
fresh review (review-server: Review mode); every other used invite is refused.

#### Scenario: Valid invite
- **WHEN** a device registers with an unused, unexpired invite and a certificate
  that verifies against the supplied identity key
- **THEN** the Pi creates the user and device and the invite can no longer be used

#### Scenario: Used invite
- **WHEN** a device registers with an invite that was already used
- **THEN** the Pi rejects it and creates nothing

#### Scenario: Used review bootstrap invite
- **WHEN** a device registers with the review server's used bootstrap invite in
  review mode, with a valid certificate for the admin
- **THEN** the Pi starts a fresh review and registers it as the admin

### Requirement: Bootstrap admin
On first start with an empty database the Pi SHALL create a single bootstrap
invite for the configured admin user id and print its invite payload to the
server log once. The user created from it SHALL be the admin. In review mode
that invite never expires (review-server: Review mode).

#### Scenario: First start
- **WHEN** the Pi starts with no users
- **THEN** it creates and prints one bootstrap invite for the admin user id

#### Scenario: Real server's bootstrap invite
- **WHEN** the real server (no `review_mode`) creates its bootstrap invite and
  the admin signs up with it
- **THEN** it expires after 7 days and a second signup with it is refused
