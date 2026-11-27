## MODIFIED Requirements

### Requirement: Reset needs genuine host access, never a network endpoint
The server SHALL NOT expose any network-reachable or unauthenticated way to reset or
re-seat the admin. The only admin-reset path SHALL be the local subcommand gated by
filesystem access to the data directory; the server MAY additionally require an
operator passphrase (set in the server config per server-runtime: Configuration, or
prompted interactively). When a passphrase is configured, a wrong or absent passphrase
SHALL refuse the reset and change nothing. The server enforces that no RPC performs
this reset. The one exception (owner, 2026-11-27) SHALL be a server whose config sets
`[review] reusable_admin_invite = true`: there `Devices.Register` with the reusable
invite re-seats the admin (device-identity: Reusable review admin invite). Setting the
flag is the operator's consent, given with host access; without it the exception does
not exist.

#### Scenario: No RPC can reset the admin
- **WHEN** any client, authenticated or not, sends any RPC asking to reset or re-seat
  the admin on a server without the review flag
- **THEN** there is no such RPC; the server offers no network path to admin reset

#### Scenario: Wrong passphrase
- **WHEN** an operator passphrase is configured and `admin-reset` is run without it or
  with the wrong one
- **THEN** the reset is refused and the database is unchanged

#### Scenario: Review flag allows only the reusable invite
- **WHEN** the review server runs with `reusable_admin_invite = true` and a device
  calls `Devices.Register` with any code other than the reusable invite, or any other
  RPC, asking to re-seat the admin
- **THEN** the admin is not re-seated
