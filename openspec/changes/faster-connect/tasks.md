## 0. Before starting

- [x] 0.1 Base on latest `main` (the DMs work touched `client/` and the generated
      bridge field); rebase before finishing to minimise generated-file conflicts
- [x] 0.2 Owner notified that the brief's expected `.proto`/`pbgen`/server change
      is not needed — the token is already connection-independent (design D1)

## 1. Proto and shared rules

- [x] 1.1 No `.proto` change: the wire contract already supports token resume
      (design D1). `buf breaking` stays clean; `pb.rs` files are untouched. (No
      work; recorded so the proto-first step is explicit.)

## 2. Spec

- [x] 2.1 `wire-transport`: ADD "Session resume"; `client-runtime`: MODIFY
      "Connection lifecycle" for reuse-and-fallback; `openspec validate
      faster-connect --strict`

## 3. Core (`client/`)

- [x] 3.1 `net/conn.rs`: a `ResumeToken { token, expires_at, renew_at }`;
      `Session::to_resume`, `Session::resume_token`-usability (`now < renew_at`),
      and `Conn::resume` to re-bind a token to a new connection
- [x] 3.2 `engine/mod.rs`: keep the token across drops (`self.resume`, set on
      auth and on renewal, not cleared by `drop_link`, cleared on `removed`);
      `open_link` resume branch before the full handshake; fall back on a
      `Connect` open that returns `UNAUTHENTICATED`
- [x] 3.3 End-to-end tests in `client/tests/connection.rs` against the real Pi:
      `client-runtime/connection-lifecycle` — a valid token is reused on
      reconnect (no new session on the Pi); an expired/renewal-due token falls
      back to the full handshake (a new session); a token the Pi no longer holds
      is rejected on the stream and the core falls back and connects

## 4. Server (`server/`) — test only

- [x] 4.1 No code change (design D1). Tests in `server/tests/messaging_stream.rs`:
      `wire-transport/session-resume` — a token minted on one connection opens
      `Connect` on another; an expired token on `Connect` is `UNAUTHENTICATED`

## 5. Verify

- [x] 5.1 `cargo build --workspace`, `cargo test --workspace`, `cargo clippy
      --workspace --all-targets -- -D warnings`, `cargo fmt --check`
- [x] 5.2 `buf lint`, `buf breaking --against '.git#branch=main'` (clean: no proto
      change), `openspec validate faster-connect --strict`
- [x] 5.3 Report the commit list to the tracker/owner; do not push (standing rule)
