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

- [ ] 3.1 New simulator "Vyajena Review": install the app, join with the review bootstrap invite, create the review group
- [ ] 3.2 A test reviewer device joins with a review invite and chats; it sees nothing of the real group
- [ ] 3.3 README, architecture doc; tell app-release `vyajarelay.purplechaamba.com:8443`; validate --strict, tests, clippy, fmt
