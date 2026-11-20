## Why

A connect over the relay is slow because the relay is far from everyone. The
relay is a GCP e2-micro in `us-east1` (South Carolina); the Pi and every user
are in India, so each round trip goes India→US→India, about 260 ms measured
(TCP connect to the relay's 443: 0.25–0.32 s every time). A cold connect spends
four round trips plus TLS on that. Change `faster-connect` removed two of them
and `resume-across-restart` makes most reopens skip two more, but the round
trip itself is still ~260 ms. Moving the relay to India cuts it to about
20–40 ms. The owner approved the move and its cost (2026-11-20).

## What Changes

- **Relay region `asia-south1` (Mumbai).** `deploy/relay/relay.conf`:
  `REGION=asia-south1`, `ZONE=asia-south1-c`, a non-overlapping subnet range,
  same `e2-micro`, same native systemd rathole (no container at runtime).
- **Relay requirement:** "Relay host" no longer says "free-tier US region"; it
  says Mumbai. `check.sh` fails a relay outside `asia-south1`, the deploy test
  pins it.
- **Cost recorded:** `asia-south1` is not in the always-free tier. About
  $11/month all in (e2-micro ~$7.3, static IPv4 ~$3.65, disk ~$0.4), roughly
  ₹1,120 with 18% GST; about ₹600 more than today, because the IP is paid
  already.
- **No re-join, no new pin.** The relay holds no state and TLS ends on the Pi.
  Invites name `vyajarelay.purplechaamba.com`, so devices follow the DNS A record
  to the new IP. Noise key pair and rathole token are reused from
  `~/.vyajena-relay/`.
- **A runbook, run only on the owner's word** (design D3): dry run and cost first,
  the owner runs `gcloud auth login`, the owner repoints the A record, the old VM
  stays until the new path is proven.
- No `.proto`, no `pbgen`, no core, no app, no Pi code. The Pi gets a new
  `RELAY_IP` in its tunnel config at deploy time.

## Capabilities

### New Capabilities

<!-- None. -->

### Modified Capabilities

- `relay`: "Relay host" — the VM SHALL be in `asia-south1` instead of a
  free-tier US region.

## Impact

- Files: `deploy/relay/relay.conf`, `deploy/relay/check.sh`,
  `deploy/relay/create.sh` (comments), `server/tests/deploy_files.rs`,
  `README.md` (Relay), `docs/architecture-decisions.md` (diagram, relay lines).
- Money: about ₹600/month more than now (Mumbai VM). The old us-east1 VM is
  free tier and stops costing nothing; its static IP stops costing ~$3.65 when deleted.
- Real systems (owner's live group): nothing runs until the owner says so.
  Brief blip at the cut-over (DNS TTL); devices reconnect with backoff.
