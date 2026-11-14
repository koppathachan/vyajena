## MODIFIED Requirements

### Requirement: Hardening check
The project SHALL provide one command that checks the Pi over SSH and reports
each failure by name: service user, sandbox settings, file modes, listening
ports, nftables rules, SSH settings and unattended upgrades. It SHALL also warn, without failing, when a
reboot is pending: `/run/reboot-required` exists, or a kernel of the running
kernel's flavour newer than the running one is installed.

#### Scenario: Root login re-enabled
- **WHEN** `PermitRootLogin` on the Pi is changed back to allow keys
- **THEN** the check fails and names the SSH root-login setting

#### Scenario: Kernel updated, not rebooted
- **WHEN** unattended upgrades installed kernel `6.18.61+rpt-rpi-v8` and the Pi
  still runs `6.18.60+rpt-rpi-v8`
- **THEN** the check warns `kernel-reboot` naming both versions and still passes
