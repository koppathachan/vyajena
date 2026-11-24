## Why

The owner wants to publish **one** generic app that any operator can point their
own friend group at: the operator runs a server (like the owner does), a friend
installs the published app, uses an invite, and is in. No server install on the
friend's side, and no owner-specific build.

Investigation of the code on `main` turned up the important fact for this change:
**the client already carries no operator-specific server.** The invite payload
(`InvitePayload`, `proto/vyajena/v1/admin.proto`) already carries the relay
address, the server's TLS SPKI pin, the invite code, and the user id (whose
`server_id` is the server's), and the core connects and pins from exactly those
values (`client/src/client.rs` `register` → `open_conn(&invite.relay_address,
invite.tls_pin)`; `client/src/net/conn.rs`). There is no baked relay endpoint,
`server_id`, or TLS pin in `client/`, `bridge/`, or `app/lib/` — a published build
reaches whichever server the pasted invite names.

So the trust model and the first-run flow are **already in the box**: the invite
is the self-contained trust-and-config root. It names where the server is (relay
address) and proves which server it is (TLS pin + `server_id`), over the trusted
channel the invite already travels on. A fresh server already prints a bootstrap
invite for the admin on first start with an empty database (device-identity:
Bootstrap admin), so the very first operator invite needs no extra step either.

What is left to make the app genuinely self-hostable is therefore smaller than the
headline suggests, and the **owner has now resolved the one open product question**
(design D2): there is **no** pre-invite "choose a server" screen — the invite alone
names the server (option A2). So the change is:

1. **Guarantee it stays operator-agnostic.** Today nothing is baked, but nothing
   stops a future regression (a default relay, a shipped bootstrap invite, owner
   branding treated as the app's server). Make "no operator server in the build" a
   spec requirement with a guard test.
2. **Keep onboarding operator-neutral.** The onboarding copy names no single
   implied server or group, and there is no choose-server step before the invite.
3. **State the invite as the trust-and-config root** so the self-hostable model is
   written down, not just implemented.
4. **Document operator setup** so a new operator can stand up a server (and must
   configure `[blob_store]` to carry audio and images).

## What Changes

- **The published build stays operator-agnostic (client-runtime).** The build
  SHALL embed no default relay address, `server_id`, TLS pin, or bootstrap
  invite; all server identity comes at runtime from the invite. A guard test
  asserts it.
- **Onboarding copy stays operator-neutral and invite-only (app-ui).** Strings are
  operator-neutral ("the server you were invited to"), and there is **no**
  pre-invite "choose a server" step: onboarding goes straight to the invite
  (design D2, option A2).
- **The invite is the trust-and-config root (device-identity).** Make explicit
  that a generic app, which embeds no server, trusts and locates a server entirely
  from the invite (relay address + TLS pin + `server_id`), so no trust-on-first-use
  gap and no server-choice step exist (design D1, option A).
- **Single server per install** is the MVP; multi-server / multi-account in one
  install is **out of scope** (design D3).
- **Operator setup is documented** (design + a docs task): the server TOML fields
  (`server_id`, `admin_username`, `listen_address`, `public_address`,
  `database_path`, `key_dir`) and the **required** `[blob_store]` for audio and
  images, plus how a fresh server prints the bootstrap invite. No new server
  requirement — `server-runtime: Configuration`, `server-runtime: First start`,
  and `device-identity: Bootstrap admin` already specify this behaviour; this
  change only surfaces it for operators.

## Open decisions resolved

- **D1 — trust model:** resolved to **option A (invite-borne TLS pin)**, which is
  already the implementation. Option B (trust-on-first-use) is documented as the
  rejected alternative.
- **D2 — does a separate "choose server" screen earn its place:** resolved by the
  owner to **no (option A2)**. The invite already names the server, so a typed
  address would only repeat it. Onboarding stays invite-only. The previously
  recommended A1a (local pre-step) is recorded in design.md as the rejected
  alternative.

## Open questions (for the owner)

- **D5 — relay topology:** does each operator run their **own** relay, or do
  operators **share** one relay (as the owner's friends share the Mumbai relay
  today)? This is surfaced, not decided — it affects what an operator must stand
  up, but not the invite model (the invite names whatever relay address the
  operator uses either way). See design D5.

## Proto files touched

- **None.** `InvitePayload` (`admin.proto`) and `ServerInfo` (`devices.proto`)
  already carry everything this change needs, so it adds no wire fields and does
  **not** contend for field numbers with the in-flight changes (`remove-closed-dm`,
  `replace-user`, `mute-notifications`, `notification-reliability`, `video-messages`,
  `delete-messages`, `audio-calls`).
- Deferred optional nicety (not in this change): a human-readable server display
  name in `ServerInfo` (and/or the invite). Not needed for A2 (no screen shows a
  server before joining); if the owner later wants one, its field number is **TBD**,
  reconciled with the tracker at implementation.

## Capabilities

### New Capabilities

<!-- None. -->

### Modified Capabilities

- `app-ui`: new requirement "Published build is operator-neutral in the UI"
  (operator-neutral copy, no pre-invite choose-server step).
- `client-runtime`: new requirement "Published build is operator-agnostic".
- `device-identity`: "Invites" modified to state the invite as the self-contained
  trust-and-config root for a generic, self-hostable app.

## Impact

- **App (`app/lib/onboarding/`):** keep the invite-only onboarding; neutralise any
  owner-implying copy. No new screen, no routing change.
- **Core / bridge:** a guard test for "no operator server baked in". No proto
  regeneration, no wire change.
- **Docs (`README`, `docs/`):** an operator-setup section (server TOML +
  `[blob_store]` required + bootstrap-invite first start).
- **Spec-only for now** (builds serialize on the shared external-disk cargo
  target; this change is not implemented here). Hold after `openspec validate
  --strict` passes.
