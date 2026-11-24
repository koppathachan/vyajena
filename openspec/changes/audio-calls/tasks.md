## 0. Before starting

- [ ] 0.1 `dms` merged (a call rides an existing DM); `relay` and `push` in place
      (the VM and the content-free wake ping)
- [ ] 0.2 Owner approves the `relay` spec change (coturn co-located on the relay VM;
      firewall opens the TURN UDP port + media range): approved 2026-11-20
- [ ] 0.3 Coordinate the `Content` oneof field number with `audio-messages` and
      `delete-messages` before assigning `CallSignal`'s

## 1. Spike (at implementation time, on real devices — not during planning)

- [ ] 1.1 S1: `webrtc-rs` builds and runs in the core on iOS and Android with
      working mic capture, echo cancellation and audio-session routing (no CallKit);
      decide `webrtc-rs` vs a native binding; result in design S1

## 2. Proto and shared rules

- [ ] 2.1 `content.proto`: `CallSignal` body (nested oneof `CallOffer` /
      `CallAnswer` / `IceCandidate` / `CallHangup` / `CallReject`), field number
      coordinated; `calls.proto`: `Calls.GetTurnCredentials`; `buf lint`,
      `buf breaking`; `cargo run -p pbgen`
- [ ] 2.2 `protocol::calls`: `b"vyajena2-call-v1"` export label, `OFFER_TTL` (~45 s),
      `TURN_CRED_TTL` (≤ 1 h), single-active-call rule, placeholder names for the
      TURN endpoints; tests `audio-calls/call-key-binding`

## 3. Server

- [ ] 3.1 `Calls.GetTurnCredentials`: ephemeral coturn HMAC credentials from the
      shared secret (loaded from config, not the repo), TTL ≤ `TURN_CRED_TTL`,
      session-authenticated; tests `audio-calls/turn-credentials`
- [ ] 3.2 Confirm the server has no media path and no call state; the egress
      allowlist is unchanged; a call's signalling routes as any DM `APPLICATION`
      message; tests `audio-calls/media-never-through-the-server`

## 4. Core

- [ ] 4.1 `calls/` module: `webrtc-rs` peer connection, ICE/DTLS/SRTP, Opus mono +
      FEC; the MLS key export and `auth_tag` compute/verify bound to `dm_epoch`;
      tests `audio-calls/call-key-binding`, `audio-calls/epoch-bound-setup`
- [ ] 4.2 Call state machine: offer/answer/ice/hangup/reject, single active call
      (auto-`BUSY`), `OFFER_TTL` timeout → missed, RAM-only state zeroized on end;
      `CallReject{epoch_stale}` + re-offer; tests `audio-calls/single-active-call`,
      `audio-calls/no-persisted-call-state`
- [ ] 4.3 `GetTurnCredentials` client use; P2P-vs-TURN per the owner's Q1 answer;
      API and events for the UI (incoming offer, state, elapsed)

## 5. Bridge

- [ ] 5.1 Call events and `start_call` / `answer_call` / `decline_call` /
      `hang_up`; `flutter_rust_bridge_codegen generate`; bridge end-to-end test

## 6. App

- [ ] 6.1 The DM shows a call message tile with Answer/Decline (incoming), "Calling…"
      with Cancel (outgoing), and "Call ended / Missed / Declined" tiles; widget
      tests `app-ui/call-in-a-dm`
- [ ] 6.2 In-call screen: mute, hang up, elapsed time, connection state; microphone
      permission; widget tests `app-ui/in-call-screen`

## 7. Ops — coturn on the relay (GATED; nothing runs from this change)

- [ ] 7.1 Add the coturn service file, config (shared secret from
      `~/.vyajena-relay/`, placeholders), and the firewall rule (TURN UDP port +
      media range) under `deploy/relay/`; `deploy/relay/check.sh` covers them;
      `shellcheck`
- [ ] 7.2 Show the owner `deploy/relay/create.sh --dry-run` and the cost; stand
      coturn up only on the owner's explicit yes (a separate step, not this change's
      code)

## 8. Docs and check

- [ ] 8.1 `docs/architecture-decisions.md` (Scope/Future: audio calls; the relay now
      also runs coturn; the honest threat model), the `relay` spec, `openspec/config.yaml`
      context if needed
- [ ] 8.2 `cargo test --workspace`, clippy, fmt, `buf lint`, `buf breaking`,
      `flutter analyze`, `flutter test`, `shellcheck deploy/relay/*.sh`,
      `openspec validate audio-calls --strict`
