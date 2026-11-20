## 0. Before starting

- [ ] 0.1 Owner decisions read (`relay-mumbai-move-plan.md`, FINAL DECISIONS): Mumbai,
      keep e2-micro, no runtime container, reuse the Noise key and token

## 1. Proto and shared rules

- [ ] 1.1 No `.proto` change; no `pbgen`

## 2. Spec

- [ ] 2.1 `relay`: MODIFY "Relay host" (Mumbai); `openspec validate relay-mumbai --strict`

## 3. Config and checks (`deploy/relay/`, no real system touched)

- [ ] 3.1 `relay.conf`: `REGION=asia-south1`, `ZONE=asia-south1-c`, a subnet range
      that does not overlap the old one; comments say it is not free tier
- [ ] 3.2 `check.sh`: the region check wants `asia-south1`; `create.sh` comments
      carry the cost and the "copy relay.env and known_hosts first" note
- [ ] 3.3 `server/tests/deploy_files.rs`: the relay-host test pins Mumbai, the
      dry run's regional commands use `--region=asia-south1` and the VM
      `--zone=asia-south1-c`
- [ ] 3.4 `README.md` and `docs/architecture-decisions.md`: region, cost, diagram
- [ ] 3.5 `cargo test -p server --test deploy_files`, `shellcheck deploy/server/*.sh
      deploy/relay/*.sh`, `openspec validate relay-mumbai --strict`
- [ ] 3.6 Report the commit list; do not push; show the owner
      `deploy/relay/create.sh --dry-run` and the cost, and wait

## 4. Move (owner steps; nothing below runs without the owner's yes)

- [ ] 4.0 Owner: make `~/.vyajena-relay/network.conf` from `deploy/network.conf.example` (change `generic-deployment`): real addresses, new ports, the Mumbai subnet range
- [ ] 4.1 Owner: `gcloud auth login`; says go after the dry run and cost
- [ ] 4.2 Owner: lower the A record's TTL (about 5 minutes) a day ahead, if the
      DNS host allows
- [ ] 4.3 `cp ~/.vyajena-relay/relay.env ~/.vyajena-relay/relay.env.old`; same for
      `known_hosts`; `deploy/server/backup.sh`
- [ ] 4.4 `deploy/relay/create.sh` (Mumbai address, subnet, VM), `deploy/relay/deploy.sh`,
      `deploy/relay/check.sh` (only the tunnel WARN and the DNS FAIL remain)
- [ ] 4.5 Owner: repoint the A record to the new IP; `dig` shows it
- [ ] 4.6 `deploy/server/deploy.sh --no-build` (new `RELAY_IP` in the tunnel unit);
      `deploy/server/check.sh` 0 failed; `deploy/relay/check.sh` 0 failed, 0 warnings
- [ ] 4.7 Verify: TCP connect to the relay's 443 about 20–40 ms; a device connects;
      `GetServerInfo` still `purplechaamba.com`; pin unchanged
- [ ] 4.8 After a day or two with devices connected through Mumbai: owner says go,
      then delete the old us-east1 VM, address and subnet (design D3 commands)
- [ ] 4.9 Archive the change; hand-over note
