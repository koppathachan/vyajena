## 0. Before starting

- [x] 0.1 Read the owner's decisions in the tracker's notes (`relay-mumbai-move-plan.md`,
      FINAL DECISIONS): token 6 h, persist in the key store, no proto change

## 1. Proto and shared rules

- [x] 1.1 No `.proto` change: the token is already connection-independent
      (faster-connect D1); `buf breaking` stays clean, `pbgen` is not run
- [x] 1.2 `protocol/src/wire_transport.rs`: `SESSION_TOKEN_MAX_LIFETIME` 1 h → 6 h;
      doc comment says why; `protocol/tests` asserts 6 h

## 2. Spec

- [x] 2.1 `wire-transport`: MODIFY "Session authentication" (6 h); `client-runtime`:
      ADD "Resume across restart"; `openspec validate resume-across-restart --strict`
- [x] 2.2 `docs/architecture-decisions.md`: the auth line says 6 hours; resume token
      in the key store

## 3. Key store

- [x] 3.1 `KeyName::SessionToken` ("session-token") in `client/src/keystore/mod.rs`;
      `KeyName::ALL` and its tests follow
- [x] 3.2 `net/conn.rs`: `ResumeToken::encode` / `decode` (version byte, expiry and
      renewal seconds, token bytes); a malformed or unknown-version value decodes
      to `None`

## 4. Core (`client/`)

- [x] 4.1 `client.rs` `start()`: load the saved token with the device keys and pass
      it to `Engine::new`; a store error or bad value is a warning and `None`
- [x] 4.2 `engine/mod.rs`: save the token to the key store on connect (when it
      changed) and on renewal, off the engine thread; delete it in `removed()`
- [x] 4.3 `identity/keys.rs`: `create_fresh` deletes any saved token
- [x] 4.4 End-to-end tests against the real Pi: reopen reuses the token (no new
      session), renewed token is the one resumed, past renewal falls back,
      a dropped token falls back, removal deletes it, token only in the key store,
      a failing key store still connects
- [x] 4.5 Existing renewal tests move from 56 min to 5 h 56 min (6 h token, 5 min margin)

## 5. Server (`server/`) — test only

- [x] 5.1 `server/tests/session_authentication.rs`: the expiry the Pi issues is
      `SESSION_TOKEN_MAX_LIFETIME` = 6 h after issue

## 6. Verify

- [ ] 6.1 `cargo build --workspace`, `cargo test --workspace`, `cargo clippy
      --workspace --all-targets -- -D warnings`, `cargo fmt --check`
- [ ] 6.2 `buf lint`, `buf breaking --against '.git#branch=main'`, `cd app &&
      flutter analyze && flutter test`, `openspec validate resume-across-restart --strict`
- [ ] 6.3 Report the commit list to the tracker/owner; do not push (standing rule)
- [ ] 6.4 Owner: deploy the Pi (`deploy/pi/build.sh && deploy/pi/deploy.sh --no-build`,
      `check.sh` 0 failed) and ship an app build; archive after both
