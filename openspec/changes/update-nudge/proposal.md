## Why

A protocol bump can silently break old clients. The first frame on `Connect`
states the client's `protocol_version`, and a server that does not support it
closes the stream with `FAILED_PRECONDITION` (wire-transport: Protocol version
negotiation). To a user on an old app that is an opaque "can't connect" — the app
looks broken, not out of date. We just went proto **v3 → v4**; an un-updated
phone now hits exactly this wall with no explanation and no way forward.

The honest fix is for the server to tell every client, on the handshake it
already performs, the **latest** app version available and the **minimum** app
version this server still speaks to, and for the app to act on that: a soft,
dismissible "update available" nudge below the latest, and a blocking "please
update" screen below the minimum — shown *before* the failing `Connect`, so the
user sees "update me", not "broken".

No new RPC and no new round trip: two optional fields ride on the `ServerInfo`
the client already fetches with `GetServerInfo`, filled from the server's config
(the `configurable-limits` pattern: optional, operator-set, behaviour-neutral
when absent).

## What Changes

- **Proto (`proto/vyajena/v1/devices.proto`): two optional fields on
  `ServerInfo`** — `latest_app_version` (field 5) and `min_app_version`
  (field 6), each a human-readable semver string (e.g. `"1.6.0"`). Empty/unset
  means "no advisory" (the default). Additive, non-breaking; old clients ignore
  unknown fields.
- **Server fills them from config.** A new optional `[app]` TOML table with
  `latest_version` and `min_version`; each optional, validated as a dotted
  numeric version, defaulting to unset (advertised as the empty string). This
  reuses the `configurable-limits` config pattern — absent table or field is
  behaviour-neutral and the effective values are logged at startup. No secret is
  logged.
- **App compares its running version** (from the platform package info, the
  `major.minor.patch` of `pubspec` `version`, build metadata ignored) against the
  two advertised values, using a numeric component-wise semver rule:
  - running `<` `min_app_version` → a **blocking "Please update" screen** with the
    store link and no way past it on this server.
  - else running `<` `latest_app_version` → a **soft, dismissible "Update
    available" banner** linking to the store.
  - else nothing.
  The blocking state takes precedence over the banner.
- **The store link is a client-side constant** (App Store on iOS, Play Store on
  Android), never a URL from the server — a compromised or hostile server must not
  be able to redirect users to a "download our update" phishing page.
- **Fail-open on garbage.** An advertised value that is empty or not a dotted
  numeric version is ignored; the app never blocks or nags on an unparseable
  advisory.

## Open decisions resolved

- Owner (Akhil, 2026-11-26): the server advertises the latest and minimum app
  versions on `ServerInfo`, operator-configurable (defaults = unset = no
  advisory); the app shows a dismissible banner below latest and a blocking
  screen below min. This is the agreed remedy for protocol bumps (v4) breaking
  old clients with an opaque connection error.

## Proto files touched

- `proto/vyajena/v1/devices.proto` — `ServerInfo` gains optional
  `latest_app_version` (5) and `min_app_version` (6). No other `.proto`,
  no field-number contention with any in-flight change (`ServerInfo` fields 1–4
  are taken; 5–6 are new).

## Capabilities

### Modified Capabilities

- `wire-transport`: `GetServerInfo` also returns the optional latest/minimum app
  versions.
- `server-runtime`: Configuration gains the optional `[app]` table.

### New Capabilities

- `app-ui`: the version-compare rule and the two update-nudge UI states (banner,
  blocking screen).

## Impact

- **Proto/codegen:** `proto/vyajena/v1/devices.proto`; `buf lint`, `buf breaking`
  (additive, passes); regenerate `pb.rs` on both sides with the usual command.
- **Server (`server/`):** `server/src/runtime/config.rs` (parse + validate the
  `[app]` table; add to the startup effective-config log), `server/src/devices/mod.rs`
  (`get_server_info` fills the two fields from config), example config +
  `deploy/server/config.toml` doc comment.
- **App (`app/`):** a version-advisory check that runs on the `GetServerInfo`
  result (before `Connect`), the store-URL constants, a dismissible banner widget
  and a blocking screen; one small place in the connect/startup flow consumes the
  `ServerInfo`. No key, no network in Dart (the core already performs
  `GetServerInfo` and hands `ServerInfo` to the UI).
- **Tests:** `server/tests/` (absent `[app]` → empty fields; set values
  advertised; a bad version refused naming the field), and `app/` widget tests
  for the compare rule and both UI states.
- **Deploy:** applied on server restart; no migration, no new credentials, no
  relay change. Behaviour-neutral until the operator sets the `[app]` values.
