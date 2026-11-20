## ADDED Requirements

### Requirement: Separate review server
Store reviewers SHALL use a separate server, never the real group's: a second
instance of the same server binary on the server with its own system user, data
directory, keys, TLS pin, bootstrap invite and ids (`review@review.purplechaamba.com`).
The server enforces this: the two services share no writable path and run as
different users.

#### Scenario: A reviewer joins
- **WHEN** the review admin invites `reviewer1` and a Play reviewer joins with
  that invite
- **THEN** the reviewer becomes `reviewer1@review.purplechaamba.com` in the
  review group, and nothing about the real group (members, group name,
  messages) reaches the reviewer's device

#### Scenario: A real invite on the review port
- **WHEN** someone pastes an invite for the real group but the device dials the
  review port
- **THEN** the TLS pin does not match and the core refuses the server

### Requirement: Review route through the relay
The relay SHALL forward the review port (<review-port>) to the server's review server on
`127.0.0.1:<review-server-port>` through the same tunnel, as its own rathole service with its
own token. Review invites SHALL name `vyajarelay.purplechaamba.com:<review-port>`.

#### Scenario: Review invite address
- **WHEN** the review admin creates an invite with `Admin.CreateInvite`
- **THEN** its payload names `vyajarelay.purplechaamba.com:<review-port>` and the review
  server's pin

### Requirement: Review reset
The project SHALL provide one command that wipes and restarts only the review
Server, after the owner types the review server id, without a backup and
without touching the real server. It SHALL save the new review bootstrap
invite to an owner-only file outside the repository, never print it.

#### Scenario: Fresh review group
- **WHEN** the owner runs the review reset and types `review.purplechaamba.com`
- **THEN** the review server starts with a new pin and an empty database, and
  the real server's pin and users are unchanged

### Requirement: Review server check
The server's hardening check SHALL cover the review server like the real one
(user, sandbox, file modes, loopback port, the pin through the relay's review
port) and SHALL fail if both servers present the same TLS key.

#### Scenario: Review service down
- **WHEN** `vyajena-review.service` is stopped
- **THEN** the check fails and names the review server
