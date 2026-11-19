## Context

The Pi logs through `tracing` with an `EnvFilter`
(`EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new("info"))`; the
service sets `RUST_LOG=info`). rustls, on the server side, emits a `WARN`
("Illegal SNI extension: ignoring IP address presented as hostname") whenever a
`ClientHello` carries a literal IP in SNI. Under pinned-key TLS the Pi ignores
SNI entirely (it verifies the server key by SPKI hash, no name/CA/date), so the
warning is purely cosmetic.

**Finding (why no dialer of ours is changed):** `client/src/net/tls.rs::connect`
sends `ServerName::try_from("vyajena2-pi")` for every connection, independent of
the relay address; `deploy/pi/check.sh` connects by the relay hostname. Neither
sends an IP as SNI. The repeated warnings are external: the relay is a raw-TCP
passthrough on a public IP, so a regular prober (~33 s) and ordinary internet
scanning reach 443 and are forwarded to the Pi's TLS. There is nothing of ours
to "fix"; the owner chose to quiet the benign log.

## Goals / Non-Goals

**Goals:**
- Drop the benign rustls IP-SNI (and sibling external-scanner handshake)
  warnings from the Pi's journal.
- Keep real rustls errors, pinned-key TLS, and everything else unchanged.

**Non-Goals:**
- Changing any dialer's SNI (ours is already a valid name).
- Blocking or identifying the external prober at the network layer (nftables and
  the tunnel are unchanged; the owner is separately aware of the prober).
- Any `.proto`/wire change.

## Decisions

### D1 — One `EnvFilter` directive, `rustls::server::hs=error`

Build the default filter from a testable `runtime::logging::filter()` that takes
the env-or-`info` base and appends `rustls::server::hs=error` — the module that
logs the server-side handshake warning. This drops `WARN`/`DEBUG` from that
module (all external-handshake noise for a pinned-key server) while `ERROR` from
it still shows, and leaves every other target at `info`. The directive is added
even when `RUST_LOG=info` is set by the service, because it is appended to the
parsed filter, not a fallback.

**Alternatives considered:** suppress all of `rustls` (too broad — hides real
client/verifier errors); configure rustls to not emit the warning (not exposed
by the API); fix a dialer (there is none — see Context).

## Risks / Trade-offs

- **[Hiding a real handshake problem]** → only `WARN`/below from
  `rustls::server::hs` is dropped; a genuine handshake failure the Pi needs is an
  `ERROR` and still logs, and pinned-key verification failures are logged by our
  own verifier (`net/tls.rs`), not this module.
- **[Scanner traffic itself]** → unchanged; this is log hygiene only. The owner
  is aware of the ~33 s prober and general scanning on the relay IP.

## Open Questions

- None. (Identifying/blocking the external prober is a separate, owner-level
  decision, deliberately out of scope here.)
