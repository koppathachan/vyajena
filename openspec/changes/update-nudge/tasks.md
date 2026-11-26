## 0. Before starting

- [ ] 0.1 Owner approved the advisory (Akhil, 2026-11-26): advertise latest/min
      app version on `ServerInfo`, operator-configurable, defaults = no advisory;
      banner below latest, blocking screen below min.
- [ ] 0.2 Confirm `ServerInfo` field numbers 5 and 6 are free and contend with no
      in-flight change (`video-messages` adds to `Limits`, not `ServerInfo`).

## 1. Proto and shared rules

- [ ] 1.1 `proto/vyajena/v1/devices.proto`: add optional `string
      latest_app_version = 5;` and `string min_app_version = 6;` to `ServerInfo`,
      with doc comments (semver string; empty = no advisory). `buf lint`,
      `buf breaking` (additive), `cargo run -p pbgen` to regenerate both sides'
      `pb.rs`; tests reference `wire-transport/protocol-version-negotiation`.

## 2. Server

- [ ] 2.1 `server/src/runtime/config.rs`: parse an optional `[app]` table with
      `latest_version` and `min_version`; each optional, validated as a dotted
      numeric version (refuse to start naming the field on a bad value), defaulting
      to unset; add both to the startup effective-config log (non-secret); tests
      `server-runtime/configuration`.
- [ ] 2.2 `server/src/devices/mod.rs` `get_server_info`: fill
      `latest_app_version` / `min_app_version` from config, advertising the empty
      string when unset; tests `wire-transport/protocol-version-negotiation`.
- [ ] 2.3 Document the `[app]` table (keys, meaning, defaults) in the example
      config and `deploy/server/config.toml` / `server/local.toml` doc comments.

## 3. App

- [ ] 3.1 A pure version-compare helper: parse `major.minor.patch` (build metadata
      ignored), compare component-wise numerically, and classify a running version
      against (`latest`, `min`) into {blocking, banner, none}, blocking taking
      precedence; empty/unparseable advertised value ignored (fail-open); tests
      `app-ui/update-nudge`.
- [ ] 3.2 Store-URL constants (App Store / Play Store), platform-selected; no URL
      is ever read from `ServerInfo`; tests `app-ui/update-nudge`.
- [ ] 3.3 Consume the advisory on the `GetServerInfo` result before `Connect`:
      show the dismissible "Update available" banner (below latest) and the
      blocking "Please update" screen (below min, short-circuiting `Connect`),
      each with the store link; tests `app-ui/update-nudge`.

## 4. Docs

- [ ] 4.1 Note in `docs/architecture-decisions.md` that app-version advisory
      fields ride on `ServerInfo` and that the store link is a client constant
      (server advertises version numbers only).
