## ADDED Requirements

### Requirement: Open registration in the core
The core SHALL expose a `register_open` entry point that takes a server relay
address and TLS pin, a chosen username and an optional join code, generates the
user identity key and this device's certificate locally in the OS key store exactly
as `register` does, and calls `Devices.Register` with an empty invite code, the
chosen username's certificate and the join code. Keys SHALL NOT leave the core. On
a server not in open mode the core SHALL surface the server's `FAILED_PRECONDITION`
as a distinct "open onboarding is not available" error, not a generic failure.

#### Scenario: Core registers openly
- **WHEN** the app calls `register_open` with `127.0.0.1:8443`, the pin, username
  `alice` and no join code against an open-mode server
- **THEN** the core generates Alice's keys, registers her with `Devices.Register`,
  and she can connect with her device key

#### Scenario: Open registration against an invite-only server
- **WHEN** the app calls `register_open` against a server in invite mode
- **THEN** the core returns an "open onboarding is not available" error and saves
  no account, so the app stays unregistered and can retry or point at another
  server
