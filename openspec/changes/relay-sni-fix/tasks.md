## 1. Quiet the log

- [x] 1.1 `server/src/runtime/logging.rs`: `pub fn filter() -> EnvFilter` =
      env-or-`info` base with `rustls::server::hs=error` appended; export from
      `runtime`.
- [x] 1.2 `server/src/main.rs`: build the subscriber's `EnvFilter` from
      `runtime::logging::filter()`.

## 2. Test

- [x] 2.1 `server/tests/`: assert `runtime::logging::filter()` carries the
      `rustls::server::hs=error` directive and still enables a normal `info`
      target (spec: server-runtime/quiet-external-tls-handshake-noise).

## 3. Verify

- [x] 3.1 `cargo build/test/clippy --workspace -- -D warnings`, `cargo fmt
      --check`.
- [x] 3.2 `buf lint`, `buf breaking --against '.git#branch=main'` (clean — no
      proto change).
- [x] 3.3 `openspec validate relay-sni-fix --strict`.
- [ ] 3.4 Report the commit list to the owner; do not push without approval.
