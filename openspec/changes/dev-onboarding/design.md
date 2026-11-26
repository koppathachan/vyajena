# Design: dev-onboarding

## Context

Production onboarding is invite-only with device-key identity (no passwords, no
logout). The friction for demos/testing/store-review is real, but the identity
model is an invariant. This design keeps the production path untouched and adds a
**config-gated** dev/demo mode. Every decision below protects two invariants:
(1) keys never leave the core and the server authenticates **device keys**, not
secrets it could forge; (2) the server never commits MLS membership.

## Decisions

### D1 — A config flag, production default off; no build-time guard

The mode lives in an optional `[onboarding]` server TOML table. Absent table or
`mode = "invite"` ⇒ today's behaviour, byte-for-byte. The owner chose a plain
config flag over a build-time guard (both were on the table): operators of a
dev/demo server opt in deliberately, and the server makes the choice visible two
ways so it is never silent:

- a loud **startup WARNING** log line when `mode = "open"`, and
- `ServerInfo.onboarding.mode = OPEN`, which clients can display.

Rejected: a release-build guard that refuses to honour the flag. More code and a
proto/app contract for "is this a release build" we do not otherwise need; the
owner judged the WARNING + advertisement sufficient for a self-hosted server.

### D2 — "Login with password" = default-admin auto-claim, **not** a password

The owner's phrasing was "allow login with password." Workshopped to the real
need: *the admin device is routed straight into the small configurable server,
then the admin does everything else.* Resolution:

- In `open` mode the admin (the user `admin_username` names) self-registers by
  **choosing the admin username** — no bootstrap-invite log-scrape. The server
  marks that user the admin exactly as today (`is_admin` is computed from
  `admin_username`; nothing new).
- There is **no password credential anywhere.** Identity stays a device-generated
  Ed25519 key in the OS key store; the device certificate is what the server
  verifies.

**Rejected alternative — password-based recovery/login.** A password that lets the
same person re-enter their account on a fresh install (no QR device-link) requires
escrowing a key derived from the password, and a password the server checks lets
the server authenticate as the user. Either **breaks E2EE** (`device-identity:
User identity key and device certificates`). Out of scope, and must never reach the
production path. This change introduces no code on that path.

### D3 — Open / join-code registration, without weakening E2EE

In `open` mode a first device registers with **no invite**. What changes and what
does not:

- **Unchanged:** the device generates its identity key and a device certificate
  locally; the server verifies the certificate against the supplied identity key;
  the user+device row is created atomically; subsequent auth is device-key
  challenge-response. The E2EE identity chain is identical to the invite path.
- **Changed:** the *authorisation to create a new user* no longer comes from an
  admin-minted single-use invite. It comes from the mode being `open`, optionally
  guarded by a shared `join_code`, and bounded by `max_users`.
- **The username** is taken from the verified certificate (`cert.username`); the
  device picks it. No new username field is needed on the wire — the certificate
  already binds `username@server_id`.

The join code is a **capability to register**, not an identity. It never signs
anything, never gates message access, is not stored per-user, and is irrelevant
once a device holds its keys. If `join_code` is unset, registration is fully open,
bounded only by `max_users`. Enforced by the **server**.

### D4 — Maximum-users cap

`max_users` (optional, in `[onboarding]`) caps the number of **active** users
(`removed_at IS NULL`). The server rejects creating a **new** user past it with
`RESOURCE_EXHAUSTED`. A re-invite/re-key of an existing user, a removed-user
re-signup, and device-linking are **not** new users and are never blocked by the
cap. Applies in both modes (safe in production). Enforced by the **server**.

### D5 — The MLS lobby-add still requires an admin device (invariant, unchanged)

Self-registration gives a device an **account**, not **lobby membership**. Adding
a member to the lobby is an MLS commit, and only an **admin device** may commit it
(commit-authority invariant); the server sees only ciphertext and can never do it.
So in `open` mode a self-registered member still shows "waiting for the admin's
device" until an admin device commits the Add (`app-ui: Join from an invite`,
scenario "Waiting to be added") — exactly as in invite mode. For the demo the
admin device is the live device, so this is free. **Fully serverless member
onboarding is out of scope because it is architecturally impossible**, not merely
unbuilt.

### D6 — Proto: `RegisterRequest.join_code`, `ServerInfo.Onboarding`

```proto
message RegisterRequest {
  string invite_code = 1;                 // empty in open mode
  bytes identity_public_key = 2;
  SignedDeviceCertificate certificate = 3;
  string join_code = 4;                   // open mode: the shared secret, if required
}

message ServerInfo {
  // ... existing fields 1-4 ...
  Onboarding onboarding = 5;
}

message Onboarding {
  enum Mode { MODE_UNSPECIFIED = 0; MODE_INVITE = 1; MODE_OPEN = 2; }
  Mode mode = 1;
  bool join_code_required = 2;            // true when a join_code is configured
  uint32 max_users = 3;                   // 0 = no cap advertised
  string admin_username = 4;              // so the app can offer "join as the admin"
}
```

The server always sets an explicit `MODE_INVITE`/`MODE_OPEN`. `MODE_UNSPECIFIED = 0`
is only what an older server (no `onboarding` message) reads as, and the client
treats it as invite mode — so the default path is unchanged. The enum names carry
the `MODE_` prefix and `_UNSPECIFIED` zero value that buf STANDARD lint requires.
`join_code` is never logged.

### D7 — App: ServerInfo-driven first-run screen

The app reads `ServerInfo.onboarding` after it has a server address (the dev-mode
flow needs an address up front; see the demo harness). When `mode == OPEN`:

- offer "pick a username" + a join-code field shown only if `join_code_required`;
- when no admin is active, offer one tap "join as the admin (`admin_username`)",
  which pre-fills the admin username; `auto-lobby` then creates/enters the lobby;
- otherwise call the core's `register_open`.

When `mode == INVITE` the screen is today's paste-an-invite (`app-ui: Join from an
invite`), unchanged. How the app first learns the dev server's address (it embeds
none — `self-hostable`) is a demo-harness concern, not a production one: the
simulator demo passes it in; a general "type a dev server address" screen is a
possible follow-up, out of scope here.

## Registration flow (open mode)

```mermaid
sequenceDiagram
    participant Dev as New device (core)
    participant Srv as Server
    Dev->>Srv: GetServerInfo
    Srv-->>Dev: ServerInfo{onboarding: OPEN, join_code_required, max_users, admin_username}
    Note over Dev: user picks username (or "join as admin"),<br/>enters join code if required
    Dev->>Dev: generate identity key + device cert for username@server_id
    Dev->>Srv: Register{invite_code:"", join_code, identity_public_key, certificate}
    alt mode != open
        Srv-->>Dev: FAILED_PRECONDITION (open mode is off)
    else join_code required and wrong
        Srv-->>Dev: PERMISSION_DENIED
    else active users >= max_users (new username)
        Srv-->>Dev: RESOURCE_EXHAUSTED
    else username taken by an active user
        Srv-->>Dev: ALREADY_EXISTS
    else ok
        Srv->>Srv: verify cert, create user+device atomically
        Srv-->>Dev: RegisterResponse
        Note over Dev: authenticates with its device key;<br/>if admin, auto-lobby creates the lobby;<br/>else waits for an admin device to add it
    end
```

## Demo harness (simulator)

A throwaway local server in a temp dir (the `app/integration_test/run.sh`
pattern), with a config like:

```toml
server_id = "demo"
admin_username = "akhil"
listen_address = "127.0.0.1:8443"
database_path = ".demo/server.sqlite"
key_dir = ".demo/keys"
[onboarding]
mode = "open"
join_code = "letmein"      # optional; omit for fully open
max_users = 10
```

This never touches `server/local.toml` (which would consume the owner's bootstrap
invite) nor the real/review servers. The simulator runs the app against
`127.0.0.1:8443`, joins as the admin through the simplified screen, and lands in
the lobby.

## Spikes

None required; no risky unknowns. The register path, `ServerInfo`, bootstrap and
`auto-lobby` are all understood from the current code.
