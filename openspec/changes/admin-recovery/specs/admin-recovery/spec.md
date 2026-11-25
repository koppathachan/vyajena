## ADDED Requirements

### Requirement: Operator break-glass reset
The server SHALL provide a local `server admin-reset --config <file.toml>`
subcommand that returns a locked-out admin to a re-bootstrappable state. Its
authority SHALL be host access to the server's data directory: it SHALL load the
given config, open that database directly, and SHALL refuse to run if it cannot open
the configured database. In one transaction it SHALL mark the configured admin user
removed (set `users.removed_at`), revoke the admin's existing devices, and revoke the
admin's unused invites. It SHALL NOT mint an invite, start a listener, or change any
other user, group, DM, the TLS or server keys, or any stored envelope. The operator
enforces this (it runs only with host access).

#### Scenario: Operator runs the reset on the box
- **WHEN** the operator runs `server admin-reset --config server.toml` on the host,
  with the admin locked out
- **THEN** the admin user is marked removed, the admin's devices and unused invites
  are revoked, and the command mints no invite

#### Scenario: Blast radius is one user
- **WHEN** the operator runs `admin-reset` on a server that also has `alice@chat` and
  `bob@chat` and a named group
- **THEN** only the admin user is changed; `alice@chat`, `bob@chat`, the group, all
  DMs, the TLS pin and the server keys are left exactly as they were

#### Scenario: No host access, no reset
- **WHEN** `admin-reset` is run where it cannot open the configured database (wrong
  path, or no permission on the 0600 file)
- **THEN** it exits with an error and changes nothing

### Requirement: Reset needs genuine host access, never a network endpoint
The server SHALL NOT expose any network-reachable or unauthenticated way to reset or
re-seat the admin. The only admin-reset path SHALL be the local subcommand gated by
filesystem access to the data directory; the server MAY additionally require an
operator passphrase (set in the server config per server-runtime: Configuration, or
prompted interactively). When a passphrase is configured, a wrong or absent passphrase
SHALL refuse the reset and change nothing. The server enforces that no RPC performs
this reset.

#### Scenario: No RPC can reset the admin
- **WHEN** any client, authenticated or not, sends any RPC asking to reset or re-seat
  the admin
- **THEN** there is no such RPC; the server offers no network path to admin reset

#### Scenario: Wrong passphrase
- **WHEN** an operator passphrase is configured and `admin-reset` is run without it or
  with the wrong one
- **THEN** the reset is refused and the database is unchanged

### Requirement: Recovered admin re-bootstraps with a fresh identity
After an operator reset marks the admin removed, the admin SHALL regain authority
through the existing bootstrap and removed-user machinery, not a new path. The next
start SHALL re-mint one bootstrap invite for the admin (device-identity: Bootstrap
admin), the admin SHALL register a new device from it, and `Devices.Register` SHALL
take the removed-user re-key path: the admin's `identity_public_key` is replaced with
the new key, `removed_at` is cleared, and the admin's old devices stay revoked. The
server enforces this.

#### Scenario: Admin re-registers with a new identity key
- **WHEN** after `admin-reset` the server restarts, prints the re-minted bootstrap
  invite, and the admin's new device registers with it and a fresh identity key
- **THEN** the server installs the new identity key for the admin, clears
  `removed_at`, and the admin can call Admin RPCs again

#### Scenario: Old admin devices stay locked out
- **WHEN** a pre-reset admin device whose keys survived tries to authenticate after
  recovery
- **THEN** it fails with `PERMISSION_DENIED`, because the reset revoked it and its old
  identity key is no longer the pinned one

### Requirement: Admin re-key forces peer re-trust
Each member's client SHALL present a clear safety-number-changed prompt naming the
admin ("⚠️ <admin>'s safety number changed — re-trust?") when the recovered admin's
pinned identity key changes (key-directory: Clients verify the identity chain), and
SHALL NOT treat the admin as a trusted/verified identity until the device owner
acknowledges it. Because the recovered admin re-keys, every member still has the
admin's old identity key pinned, so this change is seen by all of them. The client
SHALL NEVER silently auto-accept the changed key as trusted. This bare acknowledgement
is the minimum trust handling; the quorum and badge mechanisms are out of scope
(deferred to trust-verification). The client enforces this.

#### Scenario: Member sees the re-trust prompt
- **WHEN** the recovered admin's new device is added to a group a member is in, and
  the member's client learns the admin's new identity key
- **THEN** the member's client pins the new key to keep decrypting, marks the admin
  unverified, and shows "⚠️ <admin>'s safety number changed — re-trust?"

#### Scenario: No silent auto-accept
- **WHEN** the admin's pinned identity key changes on a member's client
- **THEN** the client does not present the admin as verified until the owner
  acknowledges the change, and never auto-accepts it silently
