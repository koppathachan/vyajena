## 1. Deploy files and tests

- [x] 1.1 `deploy/pi/review.toml`, `deploy/pi/vyajena-review.service`; `harden.sh` makes the `vyajena-review` user
- [x] 1.2 Tunnel and relay: `REVIEW_PORT`, review service in `tunnel.toml` and `server.toml`, `review-token` in `secrets.sh` and `render.sh`, the `vyajena-relay-review` rule in `create.sh`
- [x] 1.3 `deploy/pi/deploy.sh` installs both servers, `--reset-review`; `deploy/pi/check.sh` and `deploy/relay/check.sh` cover the review server
- [x] 1.4 `server/tests/deploy_files.rs` (review-server and the modified relay/pi-deployment requirements); shellcheck clean

## 2. Rollout (owner yes first)

- [x] 2.1 Owner: yes to `create.sh --dry-run` (one new free firewall rule) and a short tunnel restart for the real group
- [x] 2.2 `secrets.sh`, `create.sh`, `deploy/relay/deploy.sh`, `deploy/pi/deploy.sh --no-build`
- [x] 2.3 Both checks 0 failed; the real pin unchanged; review pin through `:8443`

## 3. Review admin and hand-off

- [x] 3.1 New simulator "Vyajena Review": install the app, join with the review bootstrap invite, create the review group
- [x] 3.2 A test reviewer device joins with a review invite and chats; it sees nothing of the real group
- [x] 3.3 README, architecture doc; tell app-release `vyajarelay.purplechaamba.com:8443`; validate --strict, tests, clippy, fmt

## 4. Review mode (design D6)

- [x] 4.1 Config `review_mode` (optional, default false); only `deploy/pi/review.toml` sets it
- [x] 4.2 Review bootstrap invite never expires; `Devices.Register` accepts it again and starts a fresh review (wipe all but `meta`, the bootstrap invite and `sequence`; close old streams)
- [x] 4.3 `Admin.CreateInvite` cap in review mode: 1 admin + 1 client, `FAILED_PRECONDITION`
- [x] 4.4 `server/tests/review_mode.rs`: fresh review, pin kept, old devices out, cap, real server unchanged; `deploy_files.rs` config check
- [ ] 4.5 Owner yes → merge, `deploy/pi/build.sh && deploy/pi/deploy.sh --no-build`, `--reset-review`, `deploy/pi/check.sh` 0 failed; app-release gets the invite's form (never the invite)
