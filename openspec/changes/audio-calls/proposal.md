## Why

The architecture doc lists voice and video under "Future". `audio-messages` adds
recorded voice *notes*; this change is the separate, real-time feature: **1-on-1
end-to-end-encrypted voice calls** between two members who already share a DM. The
owner approved planning it (2026-11-20, relayed by the tracker). This is a planning
change only — proposal, design, specs; no code, no deploy, no `gcloud`.

A draft the owner supplied had the right core but did not fit this repo (it
hardcoded real infrastructure, cited capabilities and sources that do not exist
here). This proposal keeps the sound core and corrects it to our invariants and
real capabilities.

## What Changes

- **A call is signalled inside an existing DM** (owner, 2026-11-20). The caller and
  callee must already share a DM (change `dms`); this change does not open one.
  Call setup messages (`CallOffer`, `CallAnswer`, `IceCandidate`, `CallHangup`,
  `CallReject`) travel as ordinary OpenMLS-encrypted `Content` bodies over the
  existing `Messaging.Connect` stream, in that DM.
- **No ringing, no CallKit/PushKit for the MVP** (owner, 2026-11-20). An incoming
  call shows up as a **regular message in the DM with Answer and Decline buttons**,
  and the ordinary content-free "New message" wake ping (message-delivery:
  Content-free wake pings) — unchanged. There is no VoIP push, no native call UI
  and no OS ring in this change.
- **The media channel is bound to the MLS identity.** The DTLS-SRTP fingerprint is
  authenticated with a per-call secret exported from the DM's MLS epoch:
  `mls_call_auth = DM.export_secret("vyajena2-call-v1", call_id, 32)`, and the
  offer/answer carry `auth_tag = HMAC-SHA256(mls_call_auth, dtls_fingerprint)`,
  which the other side verifies before media flows. A TURN relay or network
  attacker that swaps DTLS keys is detected.
- **Media is WebRTC, in the Rust core, never through the home server** (owner's
  invariant: the server sees ciphertext only). `webrtc-rs` in `client/` owns every
  UDP/ICE/DTLS socket and key; Flutter only draws the in-call screen. Audio is
  Opus ~48 kbps mono with in-band FEC. Media goes **peer-to-peer when NAT allows,
  else TURN-relayed**; it never traverses the Pi server.
- **TURN server: coturn co-located on the existing relay VM** (owner, 2026-11-20),
  **gated** behind `deploy/relay/create.sh --dry-run` + a shown cost + the owner's
  yes; **no deploy happens from this change**. The repo carries **placeholders
  only** — no real host, port, region or address (invariant: real addresses live
  in `~/.vyajena-relay/network.conf`). Ephemeral HMAC TURN credentials (TTL ≤ 1 h)
  are issued by a new `Calls.GetTurnCredentials` RPC; the server holds the coturn
  shared secret.
- **One call at a time** (owner, 2026-11-20): a device with an active or ringing
  call auto-answers a second offer with `CallReject{reason = BUSY}`.
- **No call history persisted.** Call state (keys, ICE, sockets) is RAM-only and
  zeroized on hangup or on a ~45 s unanswered-offer timeout. The DM keeps only the
  call's message tiles ("Call ended", "Missed call") as ordinary content, no media
  or keys.
- **Audio-only, 1:1.** No video, no group calls.
- **Out of scope:** video; group calls; CallKit/PushKit ringing; call history;
  call recording; a second TURN backend; the actual coturn deployment (gated,
  separate owner step).
- Details and decisions: `design.md`.

## Open decisions resolved (owner, 2026-11-20)

- **Anchor:** ride an **existing** DM's MLS epoch for the call key; do not auto-open
  a DM.
- **Incoming-call UX:** a regular DM message with Answer/Decline + the ordinary
  content-free notification; **no ring, no CallKit** for the MVP.
- **TURN:** coturn co-located on the relay VM, gated behind dry-run + cost + yes;
  no deploy from this change; placeholders only in the repo.
- **Scope:** audio-only, 1:1, single active call (auto-`BUSY`), no persisted
  history.

## Threat model (stated honestly, not "zero metadata")

- **Hidden:** the audio (SRTP), who is talking to whom *in content* (signalling is
  encrypted DM traffic), and the call keys (MLS-derived, in the core).
- **Observable:** that a call is happening and its timing/duration — a TURN-relayed
  call shows both endpoints and media timing to the TURN server (on the relay VM);
  a P2P call reveals each peer's IP to the other. The signalling messages are, to
  the server and push networks, ordinary encrypted DM messages (their existence and
  timing are already visible for any DM message). The design says this plainly.

## Invariant / spec interaction (owner-approved, 2026-11-20)

- **The `relay` spec says "Nothing else SHALL run on that VM or in that VPC."**
  Co-locating coturn **changes that**: the relay VM also runs coturn, and its
  firewall opens the TURN UDP port and media range. `docs/architecture-decisions.md`
  (Scope/Future, and the relay section) and the `relay` spec are updated. The "TLS
  ends on the server" and "relay keeps no state" rules are unaffected (coturn holds
  no call content and no persistent state).

## Proto files touched

- `proto/vyajena/v1/content.proto`: a new `CallSignal` body in the `Content` oneof
  (one field, a nested oneof of `CallOffer` / `CallAnswer` / `IceCandidate` /
  `CallHangup` / `CallReject`). Field number coordinated with the other pending
  `Content` changes (`audio-messages` `Audio`, `delete-messages` `Delete`):
  assign the next free number in merge order; `buf breaking` catches a clash.
- `proto/vyajena/v1/calls.proto` (new): service `Calls` with
  `GetTurnCredentials`.
- `protocol/`: the call-auth export label `b"vyajena2-call-v1"`, the ring/offer
  timeout, the single-active-call rule, the TURN credential TTL, and placeholder
  names for the TURN endpoints (resolved from `~/.vyajena-relay/network.conf` at
  deploy, never in the repo).
- All additive: `buf breaking` passes.

## Capabilities

### New Capabilities

- `audio-calls`: call signalling in a DM, the MLS-bound DTLS-SRTP key, single
  active call, no history, WebRTC media P2P/TURN never through the server, and TURN
  credentials.

### Modified Capabilities

- `message-content`: the `Content` oneof gains `CallSignal`.
- `app-ui`: the DM shows a call message with Answer/Decline and an in-call screen.
- `client-runtime`: the core owns the call's WebRTC sockets and keys; call state is
  RAM-only.
- `relay`: coturn runs on the relay VM; the firewall opens the TURN UDP port and
  media range (placeholders).

## Impact

- **Core (`client/`):** a `calls/` module owning `webrtc-rs`, ICE/DTLS/SRTP, the
  MLS key export and `auth_tag`, the call state machine (offer/answer/ice/hangup,
  single-active, timeout), Opus mono + FEC, and `GetTurnCredentials`. RAM-only
  state, zeroized.
- **Bridge and app:** a call message tile with Answer/Decline in the DM, an in-call
  screen (mute, hang up, elapsed time, connection state), microphone-permission
  handling. No native call UI.
- **Server (`server/`):** one new RPC, `Calls.GetTurnCredentials`, issuing
  ephemeral HMAC coturn credentials from the shared secret; egress allowlist
  unchanged (media never touches the server). No call content, no call state on the
  server.
- **Ops (gated, not in this change's code):** a coturn service on the relay VM and
  a firewall rule for the TURN UDP port + media range, added to `deploy/relay/`;
  stood up only behind `create.sh --dry-run` + cost + the owner's yes.
- **Depends on:** `dms` (the call rides an existing DM), `relay`/`push` (the VM and
  the wake ping). Independent of `lobby-multi-group`, `delete-messages`,
  `audio-messages` beyond coordinating the `Content` field number.
