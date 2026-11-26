## ADDED Requirements

### Requirement: Dev onboarding mode
The server SHALL read an optional `[onboarding]` TOML table with `mode`
(`"invite"` or `"open"`, defaulting to `"invite"`), an optional `join_code`
string, and an optional `max_users` positive integer. An absent table, or
`mode = "invite"`, SHALL keep today's invite-only onboarding exactly. When
`mode = "open"` the server SHALL log a WARNING once at startup stating that open
onboarding is enabled, and SHALL refuse to start, naming the bad field, if the
table is malformed or contains an unknown field. `join_code` SHALL NOT be logged.
The server enforces this.

#### Scenario: Default is invite-only
- **WHEN** the admin starts the server with no `[onboarding]` table
- **THEN** onboarding is invite-only, identical to today, and no open-mode warning
  is logged

#### Scenario: Open mode warns at startup
- **WHEN** the server starts with `[onboarding]` `mode = "open"`
- **THEN** it logs a WARNING that open onboarding is enabled and accepts
  invite-less registration

#### Scenario: Join code is never logged
- **WHEN** the server starts with `[onboarding]` `join_code = "letmein"`
- **THEN** no log line, including the startup configuration log, contains
  `letmein`

#### Scenario: Unknown onboarding field refuses start
- **WHEN** the config has an unknown field under `[onboarding]`
- **THEN** the server exits naming that field and opens no port

### Requirement: Server advertises its onboarding mode
The server SHALL include an `onboarding` message in `ServerInfo`, returned by
`Devices.GetServerInfo`, carrying the effective `mode` (`MODE_INVITE` or
`MODE_OPEN`), `join_code_required` (true only when a `join_code` is configured),
`max_users` (0 when no cap), and `admin_username`, so a client can render the
correct first-run screen without guessing. The server SHALL always set an explicit
`mode`; the enum's zero value `MODE_UNSPECIFIED` is only what an older server with
no `onboarding` message reads as, and a client SHALL treat it as invite mode. The
server enforces this.

#### Scenario: Open server advertises open mode
- **WHEN** a client calls `Devices.GetServerInfo` against a server in open mode
  with a join code configured and `max_users = 10`
- **THEN** `ServerInfo.onboarding` reports `mode = MODE_OPEN`, `join_code_required =
  true`, `max_users = 10` and the configured `admin_username`

#### Scenario: Invite server advertises invite mode
- **WHEN** a client calls `Devices.GetServerInfo` against a default server
- **THEN** `ServerInfo.onboarding.mode` is `MODE_INVITE`
