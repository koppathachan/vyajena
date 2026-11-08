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

MVP, in design. The `vyajena.v1` wire contract and the shared `protocol` crate
are in place (OpenSpec change `core-wire-protocol`); the Pi server, Rust core
and Flutter app come next. See the open decisions in the architecture doc.
