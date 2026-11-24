## MODIFIED Requirements

### Requirement: Signup with an invite
A user's first device SHALL register by calling `Devices.Register` with an invite
code, the new user identity public key and a device certificate for itself. The
Server SHALL create the user and the device atomically and mark the invite used. A
device that already holds a local account SHALL NOT be able to register over it; the
core SHALL first return the device to a clean no-account state (client-runtime: Device
reset), after which `Devices.Register` onboards it normally against the server's
removed-user reset or new-user path. The core enforces the "reset before register"
rule; the server treats the call as a fresh registration.

#### Scenario: Valid invite
- **WHEN** a device with no account registers with an unused, unexpired invite and a
  certificate that verifies against the supplied identity key
- **THEN** the server creates the user and device and the invite can no longer be used

#### Scenario: Used invite
- **WHEN** a device registers with an invite that was already used
- **THEN** the server rejects it and creates nothing

#### Scenario: Re-onboarding after reset
- **WHEN** Bob's phone was removed, the user resets it (client-runtime: Device reset),
  and it then registers with a fresh invite for `bob@chat`
- **THEN** the phone mints fresh device keys and a fresh identity key, the server's
  removed-user reset path accepts the registration, and the device onboards as
  `bob@chat` instead of the core failing with `AlreadyRegistered`

## ADDED Requirements

### Requirement: Account-removal notice
The server SHALL define a `ServerFrame` variant that tells a device its user was
removed from the server (an account-level notice, distinct from the per-group
inactivity `RemovedNotice` and from `DmClosed`/`GroupClosed`). On `Admin.RemoveUser`
the server SHALL deliver it on each of the removed user's own live `Connect` streams,
before the revoke closes the stream, so an online device learns of the removal
in-band. Delivery SHALL be best-effort: the server SHALL NOT delay or weaken the auth
revoke to guarantee the notice, because a revoked device cannot authenticate again;
the frame carries no message content and needs no acknowledgement. The server
enforces delivery within the removal transaction.

#### Scenario: Online device gets the notice
- **WHEN** the admin removes Bob while Bob's phone is connected
- **THEN** the server sends Bob's phone the account-removal notice on its live stream
  before closing it

#### Scenario: Revoke is not delayed for the notice
- **WHEN** the admin removes Bob while none of Bob's devices are connected
- **THEN** the server revokes Bob's devices immediately without waiting to deliver any
  notice, and Bob's devices still fail their next `Devices.Authenticate`
