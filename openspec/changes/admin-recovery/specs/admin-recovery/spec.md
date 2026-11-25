## ADDED Requirements

### Requirement: Operator break-glass reset
The server SHALL provide a local `server admin-reset --config <file.toml>`
subcommand that returns a locked-out admin to a re-bootstrappable state. Its
authority SHALL be host access to the server's data directory: it SHALL load the
given config, open that database directly, and SHALL refuse to run if it cannot open
the configured database. In one transaction it SHALL mark the configured admin user
removed (set `users.removed_at`), revoke the admin's existing devices, revoke the
admin's unused invites, and re-seed the admin-owned system lobby (see "Break-glass
re-seeds the admin's lobby"). It SHALL NOT mint an invite, start a listener, or change
any other user's account, any DM, any named group, or the TLS or server keys. The
operator enforces this (it runs only with host access).

#### Scenario: Operator runs the reset on the box
- **WHEN** the operator runs `server admin-reset --config server.toml` on the host,
  with the admin locked out
- **THEN** the admin user is marked removed, the admin's devices and unused invites
  are revoked, and the command mints no invite

#### Scenario: Only the admin account and its lobby are touched
- **WHEN** the operator runs `admin-reset` on a server that also has `alice@chat` and
  `bob@chat`, a named group and DMs
- **THEN** the admin's account is reset (removed, its devices and unused invites
  revoked) and the system lobby is re-seeded for re-creation; `alice@chat` and
  `bob@chat` keep their accounts and devices, every DM and named group, the TLS pin
  and the server keys are left exactly as they were

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

### Requirement: Break-glass re-seeds the admin's lobby
The reset SHALL re-seed the admin-owned system lobby in the same transaction, because
the lobby's only committer is the admin: once the reset revokes the admin's devices, a
recovered admin's new device could neither rejoin nor re-key the existing lobby, and
the stale old leaf would block a fresh one. It SHALL drop the lobby's server state —
its queued envelopes and their deliveries, the OpenMLS public state, the member rows,
the server's pending Removes and the gap notices — and delete the lobby's `mls_groups`
row, so the recovered admin's first `Admin.CreateGroup` seeds a fresh lobby at epoch 0
unobstructed by the old revoked admin leaf or any stale member leaf. It SHALL mark
every lobby member's device as waiting for its first add again (clear `was_member`), so
the server re-announces each non-revoked member (group-membership: Automatic add of new
users) and the recovered admin re-adds it with a fresh Welcome on the member's existing
identity key. The server SHALL surface these waiting members to the admin when the admin
re-creates the lobby (`Admin.CreateGroup`), not only when an admin device connects, so
recovery does not depend on a manual admin reconnect; this re-announcement SHALL be
idempotent and never announce a device that is already a lobby member. Pre-reset lobby
history is lost for the recovered admin's new device
(history on the lost device is unrecoverable regardless); members keep their own local
history. The reset SHALL NOT touch DMs or named groups. When no lobby exists yet, the
re-seed SHALL be a no-op. The server enforces this.

#### Scenario: Recovered admin re-creates the lobby
- **WHEN** after `admin-reset` the recovered admin registers a new device and calls
  `Admin.CreateGroup`
- **THEN** no existing system lobby blocks it, and the server seeds a fresh lobby at
  epoch 0 with the new admin device as the only member

#### Scenario: Members are re-announced and rejoin
- **WHEN** the recovered admin's device re-creates the lobby
- **THEN** each non-revoked former member is announced as waiting, the admin re-adds
  it, and the member's client rejoins from the Welcome and shows the admin re-trust
  prompt (admin-recovery: Admin re-key forces peer re-trust)

#### Scenario: Members surface without an admin reconnect
- **WHEN** the recovered admin's device connected before the lobby existed and then
  calls `Admin.CreateGroup`, so the connect-time waiting announce was missed
- **THEN** `CreateGroup` re-announces the waiting members to the admin, which re-adds
  them without any manual admin reconnect, and a member already in the lobby is not
  announced again

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
