## Why

Onboarding vyajena is deliberately heavy: identity is device-generated keys in the
OS key store, there are **no passwords and no logout**, and a new user joins only
through a **single-use invite** the admin mints with their device online
(`device-identity`). That is the right production posture, but it makes three
things painful:

- **Demos and local testing.** Standing up a usable instance means starting a
  server, scraping the printed **bootstrap invite** out of its log
  (`device-identity: Bootstrap admin`), pasting it into the app, then minting a
  per-user invite for every extra participant.
- **Store review.** A reviewer cannot self-serve; someone must hand them a live
  invite and keep an admin device online.
- **The admin's own first run.** Even the admin has to copy a code out of a log
  file before the app will let them in.

The owner (Akhil) asked to "allow login with password" to cut this friction. In
workshop we resolved what that must actually do (design **D2**): it is **not** a
password credential and **not** account recovery — it is "the admin device is
routed straight into the small configurable server, then the admin runs
everything." vyajena's identity model stays untouched; nothing is escrowed and no
server-checked password is ever introduced, because either of those would let the
server impersonate a user and **break the E2EE identity guarantee**
(`device-identity: User identity key and device certificates`).

This change therefore adds a **config-gated dev/demo onboarding mode**, **off by
default**, that removes the invite dance for testing without weakening the
production key-based E2EE model. With the mode off, behaviour is today's,
byte-for-byte.

## What Changes

- **A `[onboarding]` server config table, prod-default-off (server-runtime).** The
  server SHALL read an optional `[onboarding]` table: `mode = "invite" | "open"`
  (default `"invite"` = today's behaviour exactly), an optional `join_code` shared
  secret, and an optional `max_users` cap. In `"open"` mode the server logs a loud
  startup WARNING and advertises the mode, so it is never silently enabled.
- **Open / join-code registration (device-identity).** In `"open"` mode a first
  device MAY register **without an invite**, picking its own username, bounded by
  `max_users` and gated by `join_code` if one is configured. The device still
  generates its own identity key and device certificate and the server
  authenticates device keys exactly as today — **E2EE is unchanged**. The join
  code authorises *creating an account*; it is never an identity credential, never
  signs anything, and is irrelevant once the device is registered.
- **Default-admin auto-claim (device-identity).** In `"open"` mode the admin named
  by `admin_username` MAY self-register by choosing the admin username, with no
  bootstrap-invite log step; the server marks that user the admin as it does today.
  This is the resolution of "login with password" (**D2**).
- **A maximum-users cap (device-identity).** The server SHALL refuse to create a
  new user past `max_users`, in either mode, so a dev/demo server is bounded.
  Re-key/re-invite of an existing user and device-linking are not new users.
- **The server advertises its onboarding mode (server-runtime).**
  `ServerInfo` SHALL carry the effective onboarding mode, whether a join code is
  required, the `max_users` cap and the admin username, so the app can render the
  simplified first-run screen and a "join as the admin" affordance.
- **A simplified first-run screen (app-ui).** When the server advertises `"open"`
  mode the app SHALL offer a "pick a username (and join code, if required) → join"
  screen and, when no admin is active, a one-tap "join as the admin" path, instead
  of the paste-an-invite screen. Invite mode is unchanged. `auto-lobby` then lands
  the admin's device in the lobby.
- **A core open-registration entry point (client-runtime).** The core SHALL expose
  a `register_open` path that takes a server address + pin, a chosen username and
  an optional join code, generating identity and device keys locally as `register`
  does.
- **No change to the MLS membership rule.** Self-registration gets a device an
  account, **not** lobby membership: an **admin device** still commits the lobby
  Add (commit-authority invariant). Fully serverless member onboarding is
  architecturally impossible and stays out of scope (**D5**).

## Open decisions resolved

- **"Login with password" vs the E2EE identity model** (owner, this change): it is
  default-admin auto-claim, not a password/escrow path (**D2**). Password-based
  recovery is rejected and out of scope.
- **How the simplified mode is gated** (owner, this change): a server config flag,
  production default off, no build-time guard (**D1**).

## Proto files touched

- **`proto/vyajena/v1/devices.proto`** — `RegisterRequest.join_code` (new field);
  a nested `ServerInfo.Onboarding` message and field. Regenerate the committed
  `pb.rs` (`cargo run -p pbgen`) and the Flutter bridge.
