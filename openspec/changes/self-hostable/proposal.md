## Why

The owner wants to publish **one** generic app that any operator can point their
own friend group at: the operator runs a server (like the owner does), a friend
installs the published app, enters that operator's public relay DNS, uses an
invite, and is in. No server install on the friend's side, and no owner-specific
build.

Investigation of the code on `main` turned up the important fact for this change:
**the client already carries no operator-specific server.** The invite payload
(`InvitePayload`, `proto/vyajena/v1/admin.proto`) already carries the relay
address, the server's TLS SPKI pin, and the user id (whose `server_id` is the
server's), and the core connects and pins from exactly those values
(`client/src/client.rs` `register` → `open_conn(&invite.relay_address,
invite.tls_pin)`; `client/src/net/conn.rs`). There is no baked relay endpoint,
`server_id`, or TLS pin in `client/`, `bridge/`, or `app/lib/` — a published build
reaches whichever server the pasted invite names. So the trust model the owner
asked us to pick (see design D1) is **already the one in the box**: the invite
proves the server's key over the trusted channel the invite already travels on.

What is left to make the app genuinely self-hostable is therefore smaller than the
headline suggests:

1. **Guarantee it stays operator-agnostic.** Today nothing is baked, but nothing
   stops a future regression (a default relay, a shipped bootstrap invite, owner
   branding treated as the app's server). Make "no operator server in the build" a
   spec requirement with a guard.
2. **The owner's requested first-run UX.** Add a "choose server" step before the
   invite, so the generic app has a clear "which server am I joining" anchor and
   can show it, and record it as the install's single server.

The one real security decision (D1, trust model) and the one real product
decision (D2, does the separate "choose server" screen earn its place, given the
invite already names the server) are the **owner's to confirm** — both are raised
as open questions in design.md.

Scope note: "generic" here means *runtime server selection*, not re-publishing per
operator. The owner publishes one app under their own store identity
(`com.purplechaamba.vyajena`); other operators' friends install that same app and
choose a server at runtime. Renaming the bundle/app is **not** in scope.

## What Changes

- **A first-run "choose server" step (app-ui).** Before the invite, a fresh
  install lets the user type the server's public relay DNS address (host, port
  defaulting to 443), records it as the install's single server, and shows it.
  First-run flow becomes: **choose server → paste/scan invite → register.**
  Recommended shape (design D2, option A1a): the step is local — it stores the
  address and does no network call; the invite pasted next is what authenticates
  the server (its pin), and the app cross-checks the invite's relay address and
  `server_id` against the chosen server and surfaces a mismatch before joining.
- **The published build stays operator-agnostic (client-runtime).** The build
  SHALL embed no default relay address, `server_id`, TLS pin, or bootstrap
  invite; all server identity comes at runtime from the chosen server and the
  invite. A guard test asserts it.
- **Onboarding copy stops assuming the owner's one server (app-ui).** Strings are
  operator-neutral ("the server you were invited to", not a single implied group).
- **Trust model stated as a requirement (device-identity).** Make explicit that a
  generic app trusts a chosen server by the pin the invite carries (design D1,
  option A), so no trust-on-first-use gap exists.
- **Single server per install** stays the MVP; multi-server / multi-account is
  noted as a later nicety, not built (design D3).

## Open decisions resolved

- **D1 — trust model:** resolved to **option A (invite-borne TLS pin)**, which is
  already the implementation. Option B (trust-on-first-use) is documented as the
  rejected alternative. *Owner to confirm.*

## Open questions (for the owner)

- **D2 — does the separate "choose server" screen earn its place**, given the
  invite already names the server (endpoint + pin + `server_id`)? Headline
  question; see design.md. Recommended: yes, as a local pre-step (A1a).

## Proto files touched

- **None.** `InvitePayload` (`admin.proto`) and `ServerInfo` (`devices.proto`)
  already carry everything this change needs, so it adds no wire fields and does
  **not** contend for field numbers with the in-flight changes (`remove-closed-dm`,
  `replace-user`, `mute-notifications`, `notification-reliability`, `video-messages`).
- Deferred optional nicety (not in this change): a human-readable server display
  name in `ServerInfo` (and/or the invite) so the choose-server screen can show
  "Purple Chaamba" rather than a bare `server_id`. If the owner later wants it, its
  field number is **TBD**, reconciled with the tracker at implementation.

## Capabilities

### New Capabilities

<!-- None. -->

### Modified Capabilities

- `app-ui`: new requirement "Choose a server (first run)"; "Join from an invite"
  modified to sit after the choose-server step and cross-check the invite against
  the chosen server.
- `client-runtime`: new requirement "Published build is operator-agnostic".
- `device-identity`: "Invites" modified to state the trust anchor (the invite's
  pin and `server_id`) for a generic, self-hostable app.

## Impact

- **App (`app/lib/onboarding/`, routing, core service):** a new choose-server
  screen and a stored server address; the existing onboarding screen becomes the
  second step. No core wire change.
- **Core / bridge:** a small surface to record and read the chosen server for the
  install (where the account/device state already holds `relay_address`,
  `server_id`, `tls_pin`), and a cross-check hook; a guard test for "no operator
  server baked in". No proto regeneration.
- **Spec-only for now** (builds serialize on the shared external-disk cargo
  target; this change is not implemented here). Hold after `openspec validate
  --strict` passes.
