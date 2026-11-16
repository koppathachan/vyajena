## ADDED Requirements

### Requirement: Separate review server
Store reviewers SHALL use a separate server, never the real group's: a second
instance of the same server binary on the Pi with its own system user, data
directory, keys, TLS pin, bootstrap invite and ids (`review@review.purplechaamba.com`).
The Pi enforces this: the two services share no writable path and run as
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
The relay SHALL forward the review port (8443) to the Pi's review server on
`127.0.0.1:8444` through the same tunnel, as its own rathole service with its
own token. Review invites SHALL name `vyajarelay.purplechaamba.com:8443`.

#### Scenario: Review invite address
- **WHEN** the review admin creates an invite with `Admin.CreateInvite`
- **THEN** its payload names `vyajarelay.purplechaamba.com:8443` and the review
  server's pin

### Requirement: Review reset
The project SHALL provide one command that wipes and restarts only the review
server, after the owner types the review server id, without a backup and
without touching the real server. It SHALL save the new review bootstrap
invite to an owner-only file outside the repository, never print it.

#### Scenario: Fresh review group
- **WHEN** the owner runs the review reset and types `review.purplechaamba.com`
- **THEN** the review server starts with a new pin and an empty database, and
  the real server's pin and users are unchanged

### Requirement: Review server check
The Pi's hardening check SHALL cover the review server like the real one
(user, sandbox, file modes, loopback port, the pin through the relay's review
port) and SHALL fail if both servers present the same TLS key.

#### Scenario: Review service down
- **WHEN** `vyajena-review.service` is stopped
- **THEN** the check fails and names the review server

### Requirement: Review mode
A server whose config sets `review_mode = true` SHALL run in review mode; only
the review server's config sets it, and it is off by default. In review mode:
the bootstrap invite SHALL never expire and SHALL stay usable after use, so the
stores get one set of sign-in details that works for every review. Each signup
with it SHALL start a fresh review in one transaction: every user, device,
session, other invite, KeyPackage, the group state and every queue are deleted;
the server id, the bootstrap invite and the keys (so the TLS pin) are kept; the
previous review's open streams are closed; then the new reviewer is registered
as the admin. The server SHALL hold at most the admin and one other user:
`Admin.CreateInvite` SHALL be refused with `FAILED_PRECONDITION` while a user
other than the admin exists or an invite is pending.

#### Scenario: A new review
- **WHEN** a reviewer signs up with the review bootstrap invite after an earlier
  review created a group, a user and messages
- **THEN** the reviewer becomes the admin of an empty server with the same TLS
  pin and server id, can create the group, and nothing of the earlier review
  remains

#### Scenario: The previous review's devices
- **WHEN** a fresh review starts while the previous review's devices have open
  streams
- **THEN** their streams end and they can no longer authenticate

#### Scenario: One other user
- **WHEN** the review admin has invited one user and calls `Admin.CreateInvite`
  again, before or after that user signs up
- **THEN** the Pi refuses with `FAILED_PRECONDITION`, and allows it again once
  the pending invite is revoked or the user is removed

#### Scenario: Not in review mode
- **WHEN** a server without `review_mode` (the real server) is used
- **THEN** its bootstrap invite and invites stay single-use and expire after 7
  days, and there is no limit on users
