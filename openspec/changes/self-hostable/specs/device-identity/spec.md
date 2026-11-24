## MODIFIED Requirements

### Requirement: Invites
Only the admin's devices SHALL be able to create invites. An invite SHALL be
single-use, SHALL name the user id it creates, SHALL expire after 7 days unless
used, and SHALL be shared as an invite payload containing the relay address, the
server's TLS key pin, the invite code, and the user id (whose `server_id` is the
server's). The invite SHALL be the self-contained trust-and-config root for a
generic published app: it both names where the server is (the relay address) and
proves which server it is (the TLS pin and `server_id`), so a build that embeds no
server (client-runtime: Published build is operator-agnostic) needs nothing else
to join — no pre-invite "choose a server" step (app-ui: Published build is
operator-neutral in the UI). The client SHALL accept the server only if its TLS key
matches the invite's pin (wire-transport: gRPC over HTTP/2 and pinned TLS on TCP
443) and SHALL check the server's reported `server_id` against the invite's, so no
trust-on-first-use step is needed. The client enforces this. The admin SHALL be
able to list and revoke unused invites.

#### Scenario: Admin creates an invite
- **WHEN** an admin device calls `Admin.CreateInvite` for `alice@chat`
- **THEN** the server returns a single-use code bound to `alice@chat`

#### Scenario: Non-admin creates an invite
- **WHEN** a non-admin device calls `Admin.CreateInvite`
- **THEN** the server rejects it with `PERMISSION_DENIED`

#### Scenario: The invite is the whole trust-and-config root
- **WHEN** a published build with no baked server applies an invite for
  `alice@chat` whose payload carries the relay address, `server_id` `chat`, and the
  server's TLS SPKI pin
- **THEN** the core dials the relay address, accepts the server only if its TLS key
  matches the invite's pin, and confirms `ServerInfo.server_id` is `chat` before
  calling `Devices.Register` — with no first-connect window in which an
  impersonating server is trusted, and with no server chosen by the user beforehand
