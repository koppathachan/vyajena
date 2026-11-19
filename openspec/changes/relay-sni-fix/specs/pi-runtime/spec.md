## ADDED Requirements

### Requirement: Quiet external TLS handshake noise
The Pi's default log filter SHALL suppress rustls server-handshake warnings
(target `rustls::server::hs`, level `WARN` and below), so the benign
"IP address presented as hostname" notices from external scanners reaching the
relay's public port do not fill the journal, while still showing `ERROR` from
that module and leaving every other target at its normal level. This changes
logging only: pinned-key TLS verification and what may connect are unchanged.

#### Scenario: The benign SNI warning is filtered
- **WHEN** the Pi builds its default log filter and an external client presents
  an IP as SNI, which rustls would log at `WARN` under `rustls::server::hs`
- **THEN** the filter drops that line, and a normal `info` line from the server
  is still logged

#### Scenario: Pinned TLS is unchanged
- **WHEN** a device connects with the wrong TLS key
- **THEN** the Pi still rejects it and the mismatch is still reported, because
  that check is in the core's own verifier, not the filtered rustls module
