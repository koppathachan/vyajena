## MODIFIED Requirements

### Requirement: Signup with an invite
In invite mode (the default) a user's first device SHALL register by calling
`Devices.Register` with an invite code, the new user identity public key and a
device certificate for itself. The Server SHALL create the user and the device
atomically and mark the invite used. In open mode (server-runtime: Dev onboarding
mode) a first device MAY instead register with an empty invite code, as the Open
registration requirement defines; invite-based signup continues to work in both
modes.

#### Scenario: Valid invite
- **WHEN** a device registers with an unused, unexpired invite and a certificate
  that verifies against the supplied identity key
- **THEN** the server creates the user and device and the invite can no longer be used

#### Scenario: Used invite
- **WHEN** a device registers with an invite that was already used
- **THEN** the server rejects it and creates nothing

#### Scenario: Invite still works in open mode
- **WHEN** a device registers with a valid invite against a server in open mode
- **THEN** the server creates the user and device and marks the invite used, as in
  invite mode

## ADDED Requirements

### Requirement: Open registration
In open mode (server-runtime: Dev onboarding mode) a first device SHALL be able to
register with `Devices.Register` carrying an **empty** `invite_code`, a chosen
username (bound in its device certificate as `username@server_id`), the new user
identity public key and that certificate. The server SHALL verify the certificate
against the supplied identity key, take the username from the verified certificate,
and — subject to the Join code and Maximum users requirements — create the user and
device atomically, authenticating that device by its device key from then on
exactly as for an invited user. The E2EE identity chain is identical to the invite
path. In invite mode the server SHALL reject an invite-less `Devices.Register` with
`FAILED_PRECONDITION` and create nothing. The server enforces this.

#### Scenario: Open signup without an invite
- **WHEN** the server is in open mode and Alice's device registers with an empty
  invite code and a certificate for `alice@demo` that verifies against her identity
  key
- **THEN** the server creates `alice@demo` and her device and she can authenticate
  with her device key

#### Scenario: Invite-less registration is refused in invite mode
- **WHEN** the server is in invite mode and a device registers with an empty invite
  code
- **THEN** the server rejects it with `FAILED_PRECONDITION` and creates nothing

#### Scenario: Forged certificate in open mode
- **WHEN** a device registers in open mode with a certificate whose signature does
  not verify against the supplied identity key
- **THEN** the server rejects it with `PERMISSION_DENIED` and creates nothing

### Requirement: Join code
The server SHALL, when a `join_code` is configured (server-runtime: Dev onboarding
mode), accept an invite-less open registration only if the request's `join_code`
matches, and SHALL otherwise reject it with `PERMISSION_DENIED`. The join code
SHALL authorise only the creation of an account: it SHALL NOT be stored against the
user, SHALL NOT sign anything, and SHALL NOT gate any message, so it is never an
identity credential. When no `join_code` is configured, open registration needs
none. The server enforces this.

#### Scenario: Correct join code
- **WHEN** open mode has `join_code = "letmein"` and Alice registers with
  `join_code = "letmein"`
- **THEN** the server creates her account

#### Scenario: Wrong join code
- **WHEN** open mode has `join_code = "letmein"` and Bob registers with
  `join_code = "nope"`
- **THEN** the server rejects it with `PERMISSION_DENIED` and creates nothing

#### Scenario: No join code configured
- **WHEN** open mode has no `join_code` and Carol registers with an empty
  `join_code`
- **THEN** the server creates her account

### Requirement: Default-admin auto-claim
In open mode (server-runtime: Dev onboarding mode) the admin SHALL be able to
self-register by choosing the configured `admin_username`, with no bootstrap invite,
and the server SHALL treat that user as the admin exactly as it does a user created
from the bootstrap invite (`is_admin` derives from `admin_username`). No password
or escrowed secret is introduced; identity remains the device-held key. The server
enforces this.

#### Scenario: Admin joins directly in open mode
- **WHEN** the server is in open mode with `admin_username = "akhil"` and no active
  admin, and a device open-registers as `akhil`
- **THEN** the server creates `akhil@<server_id>`, `Devices.GetSelf` reports it is
  the admin, and the app creates the lobby (auto-lobby)

#### Scenario: Admin claim is still just a username
- **WHEN** a non-admin user open-registers in the same server
- **THEN** `Devices.GetSelf` reports it is not the admin

### Requirement: Maximum users
The server SHALL, when `max_users` is configured (server-runtime: Dev onboarding
mode), refuse to create a **new** user once that many users are active
(`removed_at IS NULL`), rejecting the registration with `RESOURCE_EXHAUSTED` and
creating nothing. Re-keying or re-inviting an existing user, re-signup of a removed
user, and device-linking SHALL NOT count as new users and SHALL NOT be blocked by
the cap. The cap applies in both invite and open mode. The server enforces this.

#### Scenario: Cap reached
- **WHEN** `max_users = 2`, two users are active, and a third username tries to
  register
- **THEN** the server rejects it with `RESOURCE_EXHAUSTED` and creates nothing

#### Scenario: Re-key under the cap
- **WHEN** `max_users = 2`, two users are active, and one of them re-registers from
  a re-invite (a re-key of the same user id)
- **THEN** the server accepts it, since no new user is created

#### Scenario: Linking a device under the cap
- **WHEN** `max_users = 2`, two users are active, and one links a second device
- **THEN** the server accepts the link, since no new user is created
