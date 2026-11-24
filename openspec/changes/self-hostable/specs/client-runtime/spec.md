## ADDED Requirements

### Requirement: Published build is operator-agnostic
The published client build SHALL embed no operator-specific server identity: no
default relay address, no `server_id`, no TLS SPKI pin, and no bootstrap invite.
All server identity SHALL be supplied at runtime — the relay address from the
chosen server (app-ui: Choose a server) and the TLS pin and `server_id` from the
invite (device-identity: Invites). The client enforces this; a guard test asserts
no such constant is compiled in.

#### Scenario: Fresh install reaches no server on its own
- **WHEN** the app is installed with no chosen server and no invite applied
- **THEN** the core connects to nothing, because it holds no relay address, pin, or
  `server_id` from the build

#### Scenario: Two operators, one build
- **WHEN** the same published build is given operator A's invite on one phone and
  operator B's invite on another
- **THEN** each phone connects only to its own operator's server, pinned to that
  server's key from its invite, with no shared baked default

#### Scenario: Guard against a baked server
- **WHEN** the guard test scans the built client and app for a 32-byte TLS pin
  literal, a default `relay_address`, a `server_id` constant, or an embedded
  invite payload
- **THEN** it finds none and fails the build if any is introduced
