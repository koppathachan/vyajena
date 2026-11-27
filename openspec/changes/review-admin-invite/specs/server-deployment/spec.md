## ADDED Requirements

### Requirement: Review admin invite hand-off
The server binary SHALL provide a local `server review-invite --config <file.toml>`
subcommand that prints the reusable admin invite's payload (device-identity: Reusable
review admin invite) and refuses, changing nothing, when the config does not set
`reusable_admin_invite = true` or the database cannot be opened. Its authority SHALL be
host access, like `admin-reset`; there SHALL be no RPC that returns the invite. The
project SHALL provide `deploy/server/deploy.sh --review-invite`, which runs it on the
review server over SSH and saves the payload to an owner-only (0600) file
`~/vyajena-server-backups/review-admin-invite-<UTC>.txt`, never printing it. The real
server's config SHALL never set `[review]`; the hardening check SHALL fail, naming
`real review flag`, if it does. The operator and the hardening check enforce this.

#### Scenario: Owner hands off the invite
- **WHEN** the owner runs `deploy/server/deploy.sh --review-invite`
- **THEN** the review server's reusable invite is saved to a 0600 file under
  `~/vyajena-server-backups/` and the terminal shows only the file's path

#### Scenario: Not a review server
- **WHEN** `server review-invite` runs with the real server's config
- **THEN** it exits with an error saying the review flag is off and prints no invite

#### Scenario: Real server flag caught
- **WHEN** `/etc/vyajena/config.toml` on the server contains a `[review]` table
- **THEN** `deploy/server/check.sh` fails and names `real review flag`
