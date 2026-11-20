# device-identity Specification

## Purpose
TBD - created by archiving change core-wire-protocol. Update Purpose after archive.
## Requirements
### Requirement: Federation-ready identifiers
Every user SHALL be identified as `<local>@<server_id>`. Every device SHALL be
identified by its user id plus a 16-byte device id equal to the first 16 bytes of
the SHA-256 hash of its device auth public key.

#### Scenario: Device id is derived, not assigned
- **WHEN** a device generates its auth key pair
- **THEN** it can compute its own device id without asking the server

### Requirement: User identity key and device certificates
Each user SHALL have one Ed25519 user identity key, generated on and held by
their first device in the OS key store. Every device of the user SHALL have a
device certificate, signed by the user identity key over the context string
`vyajena2-device-cert-v1`, binding the user id, the device id, the device auth
public key, the device's MLS signature public key and a creation time. The server
SHALL accept a device certificate only if it verifies against the identity key
stored for that user.

#### Scenario: Valid certificate
- **WHEN** a device presents a certificate signed by its user's registered identity
  key
- **THEN** the server accepts it

#### Scenario: Forged certificate
- **WHEN** a certificate's signature does not verify against the user's registered
  identity key
- **THEN** the server rejects it with `PERMISSION_DENIED`

### Requirement: Invites
Only the admin's devices SHALL be able to create invites. An invite SHALL be
single-use, SHALL name the user id it creates, SHALL expire after 7 days unless
used, and SHALL be shared as an invite payload containing the relay address, the
server's TLS key pin and the invite code. The admin SHALL be able to list and revoke
unused invites.

#### Scenario: Admin creates an invite
- **WHEN** an admin device calls `Admin.CreateInvite` for `alice@chat`
- **THEN** the server returns a single-use code bound to `alice@chat`

#### Scenario: Non-admin creates an invite
- **WHEN** a non-admin device calls `Admin.CreateInvite`
- **THEN** the server rejects it with `PERMISSION_DENIED`

### Requirement: Admin-owned usernames
The admin SHALL choose each user's username when creating their invite, and it
SHALL be the local part of the user id. No RPC SHALL let a user change their own
username.

#### Scenario: Signup uses the admin's username
- **WHEN** a device registers with an invite created for `alice@chat`
- **THEN** the new user's id is `alice@chat`, whatever the device sends

#### Scenario: Certificate for another user
- **WHEN** a device registers with an invite created for `alice@chat` and a
  certificate naming `mallory@chat`
- **THEN** the server rejects it with `PERMISSION_DENIED` and creates nothing

### Requirement: Signup with an invite
A user's first device SHALL register by calling `Devices.Register` with an invite
code, the new user identity public key and a device certificate for itself. The
Server SHALL create the user and the device atomically and mark the invite used.

#### Scenario: Valid invite
- **WHEN** a device registers with an unused, unexpired invite and a certificate
  that verifies against the supplied identity key
- **THEN** the server creates the user and device and the invite can no longer be used

#### Scenario: Used invite
- **WHEN** a device registers with an invite that was already used
- **THEN** the server rejects it and creates nothing

### Requirement: Bootstrap admin
On first start with an empty database the server SHALL create a single bootstrap
invite for the configured admin user id and print its invite payload to the
Server log once. The user created from it SHALL be the admin.

#### Scenario: First start
- **WHEN** the server starts with no users
- **THEN** it creates and prints one bootstrap invite for the admin user id

### Requirement: Admin awareness
`Devices.GetSelf` SHALL return the calling device's user id and certificate and
whether the caller's user is the admin, so the admin's app knows, before any
group exists, that it should create the group with `Admin.CreateGroup`.

#### Scenario: Admin's first device
- **WHEN** the device registered from the bootstrap invite calls `Devices.GetSelf`
- **THEN** the server reports that its user is the admin, and the app creates the
  group with `Admin.CreateGroup`

#### Scenario: Ordinary member
- **WHEN** a device of a user other than the admin calls `Devices.GetSelf`
- **THEN** the server reports that its user is not the admin

### Requirement: Device linking
A user SHALL be able to add a device by having the new device display its device
auth public key and MLS signature public key as a QR code, scanning it on the
device that holds the identity key, and calling `Devices.LinkDevice` with a
certificate signed there. The new device SHALL then authenticate with its own
auth key; it SHALL NOT need an invite.

#### Scenario: Linking a phone
- **WHEN** the identity-holding device uploads a valid certificate for a new device
- **THEN** the new device can authenticate and fetch its user id and certificate
  with `Devices.GetSelf`

#### Scenario: Linking without the identity key
- **WHEN** a device of the user that does not hold the identity key tries to link
  a device
- **THEN** it cannot produce a valid certificate and the server rejects the request

### Requirement: Device revocation
Any authenticated device of a user SHALL be able to revoke another device of the
same user with `Devices.RevokeDevice`. A revoked device SHALL fail authentication
from then on with `PERMISSION_DENIED`, and its open stream SHALL be closed with
`PERMISSION_DENIED`; the server enforces both.

#### Scenario: Lost phone
- **WHEN** a user revokes their lost phone from their tablet
- **THEN** the phone's next authentication fails with `PERMISSION_DENIED` and its
  open stream closes with `PERMISSION_DENIED`

### Requirement: Push token registration
A device SHALL register or replace its push token with `Devices.UpdatePushToken`,
stating the platform (FCM or APNs). A device SHALL be able to clear its token.

#### Scenario: Token rotation
- **WHEN** a device sends a new FCM token
- **THEN** the server replaces the old token and uses the new one for wake pings

