# vyajena2

End-to-end encrypted, old-school text messaging for one admin-run group.
Successor to the original `vyajena` learning experiment.

The MVP is invite-only and text-only, end-to-end encrypted with
[OpenMLS](https://openmls.tech/) (RFC 9420), and store-and-forward through a
self-hosted Raspberry Pi server. Plaintext exists only inside the Rust core on
each device — the relay and the server see only ciphertext.

## Architecture

Four components:

- **Flutter UI** — draws screens only; never opens a connection.
- **Rust core** — OpenMLS, networking, local SQLite, keys in Keystore/Keychain.
  Linked through `flutter_rust_bridge`.
- **GCP relay** — dumb TCP passthrough so no port opens at home.
- **Pi server** — routes, orders and queues ciphertext; dispatches wake pings.

Full rationale: [`docs/architecture-decisions.md`](docs/architecture-decisions.md).

## Run the server locally

Needs Rust (stable). Nothing runs as root; data stays in `server/.pi-local/`
(git-ignored).

```bash
cargo run -p server -- --config server/local.toml
```

```
first start ──> creates keys + database in server/.pi-local/
            ──> logs the TLS pin          (every start, never changes)
            ──> logs the bootstrap invite (ONCE, first start only)
            ──> listening on 127.0.0.1:8443 (TLS 1.3, HTTP/2)
```

- **TLS pin:** the log line `TLS key pin` → field `tls_spki_sha256`. Same on
  every restart.
- **Bootstrap invite:** the log line `bootstrap admin invite created` → fields
  `invite_code` and `payload` (what a client needs: relay address, pin, code).
  It creates the admin, `akhil@pi`, and is valid for 7 days.
- **Copy it the first time.** Later starts only say an invite is pending, never
  the code again.
- **Lost it, or want a clean slate?** Stop the server, then:

  ```bash
  rm -rf server/.pi-local
  ```

  The next start makes new keys (a new pin) and a new invite.
- **Settings:** `server/local.toml` (port, admin username, paths). An unknown
  or missing field stops the server with the field's name.
- **Stop:** Ctrl-C.

Run the tests (each one starts its own server in a temp folder, so they never
touch `server/.pi-local/`):

```bash
cargo test --workspace
```

## Development

This project is spec-driven with [OpenSpec](https://github.com/Fission-AI/OpenSpec).
Every non-trivial change starts as a proposal:

```bash
# propose a change
openspec list          # see active changes
# or in Claude Code:
/opsx:propose "your idea"
```

Project context, invariants and conventions live in
[`openspec/config.yaml`](openspec/config.yaml); agent workflow in
[`CLAUDE.md`](CLAUDE.md).

## Status

MVP, in progress. The `vyajena.v1` wire contract and the shared `protocol`
crate are in place (OpenSpec change `core-wire-protocol`), and the Pi server
runs locally (`pi-server-core`). Next, in order: the Rust core library, a
terminal client, relay and Pi deployment, then the Flutter app with real push.
See the open decisions in the architecture doc.
