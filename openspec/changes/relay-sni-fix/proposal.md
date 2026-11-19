## Why

The Pi's journal fills with a repeated `WARN` from rustls —
`Illegal SNI extension: ignoring IP address presented as hostname` — roughly
every 33 seconds, drowning real log lines. Investigation showed **this is not our
bug**: `client/src/net/tls.rs::connect()` always sends the fixed ServerName
`vyajena2-pi` (a valid DNS name) regardless of the address, and `deploy/pi/
check.sh` dials by hostname — neither presents an IP as SNI. The warnings come
from **external traffic** (a regular prober at ~33 s plus general internet
scanning) reaching the relay's public IP:443 and being forwarded through the
tunnel to the Pi's TLS, where rustls notes the benign IP-in-SNI and ignores it
(SNI has no role under pinned-key TLS). The owner chose to quiet the noise.

## What Changes

- **Quiet the benign rustls server-handshake warning.** Add a
  `rustls::server::hs=error` directive to the Pi's default log `EnvFilter`, so
  these external-scanner handshake warnings are dropped while real errors from
  that module still show. Pinned-key TLS and every other behaviour are
  unchanged; no `.proto`/wire change.
- No change to what connects or how TLS is verified — our client was never the
  cause.

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
- `pi-runtime`: the Pi's default log filter suppresses benign rustls
  server-handshake warnings (external scanner/prober noise) while keeping real
  errors and the pinned-key TLS behaviour.

## Impact

- `server/src/main.rs` — build the `EnvFilter` from a new testable
  `runtime::logging::filter()` that adds the `rustls::server::hs=error`
  directive.
- `server/src/runtime/logging.rs` — the helper.
- `server/tests/` — assert the default filter carries the directive (and still
  enables `info`).
- Security note (design): a regular external prober (~33 s) and general scanning
  hit the relay's public IP:443; the owner is aware. Quieting the log does not
  change who can reach the Pi (nftables + the tunnel are unchanged); it only
  stops logging the benign SNI quirk.
