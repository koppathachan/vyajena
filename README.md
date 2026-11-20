# vyajena2

End-to-end encrypted, old-school text messaging for one admin-run group.
Successor to the original `vyajena` learning experiment.

The MVP is invite-only, text and pictures, end-to-end encrypted with
[OpenMLS](https://openmls.tech/) (RFC 9420), and store-and-forward through a
self-hosted host server. Keys never leave the Rust core on each device;
decrypted text and pictures reach the Flutter UI only to be shown, never stored, logged or
sent from there. The relay and the server see only ciphertext.

## Architecture

Four components:

- **Flutter UI** — draws screens only (iOS and Android); never opens a
  connection, never stores or logs message text.
- **Rust core** — OpenMLS, networking, local SQLite, keys in the OS key store
  (Keystore/Keychain on phones, the OS keychain on desktop). Linked through
  `flutter_rust_bridge`.
- **GCP relay** — dumb TCP passthrough so no port opens at home.
- **Server** — routes, orders and queues ciphertext; dispatches wake pings.

Full rationale: [`docs/architecture-decisions.md`](docs/architecture-decisions.md).

## Run the server locally

Needs Rust (stable). Nothing runs as root; data stays in `server/.server-local/`
(git-ignored).

```bash
cargo run -p server -- --config server/local.toml
```

```
first start ──> creates keys + database in server/.server-local/
            ──> logs the TLS pin          (every start, never changes)
            ──> logs the bootstrap invite (ONCE, first start only)
            ──> listening on 127.0.0.1:8443 (TLS 1.3, HTTP/2)
```

- **TLS pin:** the log line `TLS key pin` → field `tls_spki_sha256`. Same on
  every restart.
- **Bootstrap invite:** the log line `bootstrap admin invite created` → fields
  `invite_code` and `payload` (what a client needs: relay address, pin, code).
  It creates the admin, `akhil@chat`, and is valid for 7 days.
- **Copy it the first time.** Later starts only say an invite is pending, never
  the code again.
- **Lost it, or want a clean slate?** Stop the server, then:

  ```bash
  rm -rf server/.server-local
  ```

  The next start makes new keys (a new pin) and a new invite.
- **Settings:** `server/local.toml` (port, admin username, paths). An unknown
  or missing field stops the server with the field's name.
- **Stop:** Ctrl-C.

Run the tests (each one starts its own server in a temp folder, so they never
touch `server/.server-local/`):

```bash
cargo test --workspace
```

## Run the app locally

Two phones on one Mac: an iOS Simulator and an Android emulator chatting
through your local server. Needs Flutter 3.44, Xcode with an iOS Simulator, the
Android SDK with an emulator, and the Rust phone targets (cargokit builds
`bridge/` from Xcode and Gradle):

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios \
  aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
```

```
1. local server        cargo run -p server -- --config server/local.toml
                   copy the bootstrap invite: log line "bootstrap admin
                   invite created", field payload (first start only)
2. iOS Simulator   open -a Simulator
                   cd app && flutter run -d <simulator id>
3. Android         flutter emulators --launch <emulator id>
                   adb reverse tcp:8443 tcp:8443
                   cd app && flutter run -d emulator-5554
4. first phone     paste the bootstrap invite → Check invite → Join
                   → Create the group                   (you are akhil@chat)
                   menu → Invite a user → bob → Copy or Share the text
5. second phone    paste bob's invite → Join → "Waiting for the admin's
                   device…" → the admin's phone adds it on its own → chat
```

- **Device ids:** `flutter devices`.
- **Android and 127.0.0.1:** invites name `127.0.0.1:8443`; `adb reverse`
  makes that address on the emulator reach the Mac. Run it again after the
  emulator restarts. The iOS Simulator reaches the Mac directly.
- **Pasting long text:** iOS Simulator: `xcrun simctl pbcopy <simulator id> <
  file`, then long-press the field → Paste. The Android emulator shares the
  Mac's clipboard.
- **Xcode 27.2:** never `flutter build ios --simulator` (Flutter's `lipo` step
  fails); `flutter run -d <simulator id>` builds one architecture and works.
- **The bootstrap invite is single-use.** Once your first phone has joined,
  later runs reuse that phone. A clean slate (`rm -rf server/.server-local`)
  deletes the local server's users and group.

**Shortcuts:** the steps above are in the `Makefile` (`make help` lists them),
one terminal each:

```bash
make rust-targets     # once
make server               # local server (or: make server-relayed, plus make relay)
make ios              # booted iOS Simulator
make android ANDROID_EMULATOR=<id>   # ids: make emulators; adb reverse included
```

**Reset the data** (a clean slate: new server keys, a new bootstrap invite, phones
join again). Stop the server and quit the app runs first, then reset both sides
together. A phone that kept its old identity will not connect to a server with new
keys (the TLS pin changes), and the other way round.

```bash
make server-reset           # the server from `make server`        (server/.server-local/)
make server-relayed-reset   # the server from `make server-relayed` (server/.server-local/relayed/)
make reset-ios          # uninstalls the app from the booted simulator
make reset-android      # uninstalls the app from the emulator
```

Then start the server again, copy the new bootstrap invite from its log (printed
once), and `make ios` / `make android` reinstall the app fresh. With several
simulators booted, pick one with `IOS_DEVICE=<id>` (`make devices`); a second
Android emulator is `ANDROID_DEVICE=<id>`. By hand: `rm -rf server/.server-local`,
`xcrun simctl uninstall <simulator id> com.purplechaamba.vyajena`,
`adb uninstall com.purplechaamba.vyajena`, or Erase All Content and Settings in
the Simulator. `make server-reset` keeps the relayed server's data and the other way
round; the real server is never touched (its reset is `deploy/server/deploy.sh --reset`,
which backs up first).

`make relay` is a plain TCP passthrough on 127.0.0.1:9443 to the server, for use
with `make server-relayed` (its own data in `server/.server-local/relayed/`; its
invites name port 9443, and a picture store in `server/.server-local/relayed/blobs`,
so pictures work there; `make server` has none, so pictures stay off). It stands in
for the real relay's path, not for rathole.

App tests never touch your local server or its invite:

```bash
cd app && flutter analyze && flutter test        # widget tests, a fake core
app/integration_test/run.sh <device id> [chat|images]   # the real app on a device
```

`run.sh` starts its own temporary server on a free port in a temp folder (never
`server/local.toml`), plays the admin's device on the Mac through the core
(`bridge/tests/app_peer.rs`, or `app_peer_images.rs` for `images`), and runs
`app/integration_test/` on the device as Bob. `chat`: join, added
automatically, "hi" ⏳ → ✓, "hey" received. `images`: send a picture, open the
admin's (blur, download, viewer), send one while offline (it queues, uploads,
then sends), "Resend picture". It stops the server and removes the folder when
done.

**No screenshots, no tapping:** `make test-ios` and `make test-android` run the
`images` scenario (`SCENARIO=chat` for the other) on scratch devices, a
simulator "Vyajena Test" and the emulator AVD `vyajena_test`, which the script
creates or starts if needed. The test clears the app's data on its device,
which is why it never uses the phones you chat on. About 10 s per run once the
app is built. The first run of a freshly built Mac-side peer can take about a
minute: macOS scans every new binary on its first start (the script waits
up to four minutes for it). `make test-bridge` runs the same pictures through
the bridge without a device.

## Run on the server

The real server (a Linux host, Debian 13, reached over SSH on its home network) runs the server
as `vyajena.service` on its loopback port only, and `vyajena-tunnel.service`
(rathole) dials out to the relay. Devices anywhere dial `vyajarelay.purplechaamba.com:443`;
nothing listens at home but SSH from the home network. Needs Docker Desktop on
the Mac (`build.sh` starts it), the server login with passwordless sudo, and the
relay's secrets in `~/.vyajena-relay/` (see Relay). `SERVER=user@host` overrides the
target for every script.

**Network settings.** The repository holds no real address or port. Copy
`deploy/network.conf.example` to `~/.vyajena-relay/network.conf` (mode 0600) and
fill in the host's SSH target and static address, its home network and router,
the admin username, the loopback ports, the relay's tunnel and review ports, the
relay subnet and Google's IAP range. `deploy/network.sh` loads it for every
script in `deploy/relay/` and `deploy/server/`; a missing key stops the script by name.
The port 443 devices dial and SSH's 22 are the only fixed ones.

```
build     deploy/server/build.sh        arm64 release builds in containers
                                    -> target/server/vyajena-server, target/server/rathole
deploy    deploy/server/deploy.sh       build, harden the host, install, restart
          deploy/server/deploy.sh --no-build   install the last build as is
          deploy/server/deploy.sh --rollback   back to the previous server binary
          deploy/server/deploy.sh --reset      back up, then DELETE the group (asks for "server")
check     deploy/server/check.sh        hardening + tunnel + relay pin; FAIL <name> per problem
backup    deploy/server/backup.sh       DB snapshot + keys -> ~/vyajena-server-backups/<UTC time>/
logs      ssh <user>@<server-address> sudo journalctl -u vyajena -u vyajena-tunnel -u vyajena-egress -f
```

- **Config:** `deploy/server/config.toml`, `tunnel.toml` and
  `vyajena-tunnel.service` are templates; `deploy.sh` fills in the relay's
  address and the tunnel token from `~/.vyajena-relay/`
  (`deploy/relay/render.sh`) and pipes them over SSH. The token file on the server
  is `/etc/vyajena/tunnel.toml`, root only.
- **Bootstrap invite:** a first install or a `--reset` saves it to
  `~/vyajena-server-backups/bootstrap-invite-<UTC time>.txt` (owner only) instead
  of printing it. Lost it?
  `ssh <user>@<server-address> sudo journalctl -u vyajena -o cat | grep "bootstrap admin"`.
  It creates the admin `ADMIN_USERNAME@purplechaamba.com` (`deploy/server/config.toml`) and
  is valid for 7 days.
- **TLS pin:** every deploy prints `tls_spki_sha256`. Upgrades keep
  `/var/lib/vyajena` (database and keys), so it changes only with `--reset`.
- **Reset** (`--reset`): type `server` to confirm, `backup.sh` runs first (and a
  failed backup stops everything), then the database and keys are deleted and
  the server starts fresh: new pin, new bootstrap invite, every device re-joins.
- **Hardening** (`harden.sh`, run by every deploy, idempotent): the `vyajena`
  system user, SSH keys only and no root login, nftables (only SSH from
  `<home-lan>`, inbound IPv6 dropped), unattended security updates
  without automatic reboots, mDNS off, and the static address. SSH, firewall
  and address changes arm an automatic undo for 2 minutes and keep it only
  after a new SSH connection from the Mac gets in.
- **Static address:** `<server-address>` is set on the server's `eth0` (NetworkManager),
  for SSH from the Mac. If the router hands it to another device, `check.sh`
  warns (`address-clash`); reserve `<server-address>` for MAC `<server-mac>`
  on the router, or shrink its DHCP pool.
- **Pending reboot:** updates install by themselves but never reboot;
  `check.sh` warns `reboot-required`, or `kernel-reboot` when a newer kernel
  of the running flavour is installed (the host's kernels leave no
  `/run/reboot-required`). Reboot when convenient:
  `ssh <user>@<server-address> sudo reboot`. The tunnel comes back by itself.
- **Backups** hold the server's private keys: they stay out of the repo, in
  owner-only folders.

## Push

A closed app is woken by a notification with one fixed text, "New message":
no content, sender, group or count reaches Google or Apple (change `push`).
Android: FCM HTTP v1 (Firebase in GCP project `vyajena`); iOS: APNs with a
`.p8` key the server signs with, no Firebase on iOS. The review server only logs
pings.

```
keys      ~/.vyajena-push/ (0700, files 0600; never in the repo)
            fcm-service-account.json   service account vyajena-push@vyajena.iam (FCM only)
            AuthKey_<key id>.p8        the APNs key, named as Apple downloads it
            google-services.json       read by Gradle for Android builds (not sent to the server)
deploy    deploy/server/deploy.sh          copies the two server keys to /var/lib/vyajena/push (0600)
egress    vyajena-egress.service       127.0.0.1:8445, CONNECT only to fcm.googleapis.com,
                                       oauth2.googleapis.com, api(.sandbox).push.apple.com:443
check     deploy/server/check.sh           egress sandbox + allowlist, key modes; WARN push-fcm/push-apns
logs      ssh <user>@<server-address> sudo journalctl -u vyajena | grep "wake ping"
```

- The server stays loopback-only; its pings leave through the egress proxy
  (`push.toml`: `credentials_dir`, `proxy`, APNs team and topic).
- A platform without its key only logs pings; add the key and deploy again.
- The app asks for notification permission once, at its first connect after
  joining, and registers its token through the core on every connect.
- Android builds without `~/.vyajena-push/google-services.json` have no push;
  release builds refuse to build without it (`-Pvyajena.allowNoPush=true` for
  throwaway tests).

## Relay

A GCP e2-micro (`vyajena-relay`, `asia-south1-c` Mumbai, project
`vyajena`, its own VPC) runs rathole 0.5.0 as a dumb TCP passthrough: 443
for devices, <tunnel-port> for the server's tunnel (Noise, the relay's key pinned on the
Server, plus a token). TLS ends on the server. A reserved static IPv4, named `vyajarelay.purplechaamba.com` (an A record
the owner keeps at the domain's DNS host; `check.sh` checks it),
about $3.65/month; the VM is not in the free tier (about $11/month in all,
Mumbai e2-micro + IP + disk). SSH only through Google IAP. It holds
no state: logs in RAM, no service account. Names and sizes:
`deploy/relay/relay.conf`. Every script but `secrets.sh` needs a valid
`gcloud auth login` (run it yourself).

```
secrets   deploy/relay/secrets.sh   token, Noise keys, SSH key -> ~/.vyajena-relay/ (once)
create    deploy/relay/create.sh --dry-run   print the gcloud commands
          deploy/relay/create.sh    VPC, firewall, static IP, VM (idempotent)
build     deploy/relay/build.sh     rathole for amd64 -> target/relay/rathole
deploy    deploy/relay/deploy.sh    build, harden, install, restart (--no-build, --rollback)
check     deploy/relay/check.sh     GCP + host + ports; FAIL <name> per problem
shell     deploy/relay/ssh.sh       SSH over IAP (or one command)
```

- **Secrets** live only in `~/.vyajena-relay/` (0700, files 0600): `token`,
  `noise.key`, `noise.pub`, `ssh_ed25519`, plus `relay.env` (the address) and
  `known_hosts` (the VM's host key, read from guest attributes). Keep a copy
  in the password manager; never put them in the repo.
- **Rebuild:** the VM is disposable. Delete it and run `create.sh` and
  `deploy.sh` again; keep the static address, which every invite names.
- **Rathole** is built from its crates.io source with its own lock file
  (`--locked`) in a `rust:1.79` container (its lock does not build on newer
  Rust). The server's copy comes from `deploy/server/build.sh`.

## Review server

Store reviewers (Play, App Store) get their own server, never the real group:
`vyajena-review.service` on the server (user `vyajena-review`, data in
`/var/lib/vyajena-review`, `127.0.0.1:<review-server-port>`, ids `<name>@review.purplechaamba.com`),
reached at **`vyajarelay.purplechaamba.com:<review-port>`** through the same tunnel with
its own token. Change `review-server`.

```
admin       the "Vyajena Review" iOS simulator (review@review.purplechaamba.com)
invite      on that simulator: menu -> Invite a user -> a fresh single-use invite per review
            (paste it into Play's App access form; never into the repo)
reset       deploy/server/deploy.sh --reset-review    wipes only the review server (type its id)
            the new review bootstrap invite -> ~/vyajena-server-backups/review-bootstrap-invite-<UTC>.txt
check       deploy/server/check.sh                    covers both servers; fails if they share a pin
```

- `deploy/server/deploy.sh` installs and upgrades both servers; `backup.sh` skips
  the review server (no real data).
- After a review: remove the reviewer (menu -> Remove a user) or `--reset-review`.

## Release

The app ships privately for now: Play internal testing, and TestFlight
internal testing once the Apple account exists (change `app-release`).
[`docs/release.md`](docs/release.md) lists every owner step. In short:

```bash
app/store/build-aab.sh                      # signed bundle, upload key from the Keychain
app/store/build-ipa.sh                      # signed App Store IPA (team in Xcode)
app/store/release-check.sh                  # every line must PASS before an upload
```

Keys never enter the repo or CI. Store texts, privacy answers, icons
(`app/store/icons.sh`) and screenshots (`app/store/screenshots.sh`, fake core
only) live in `app/store/`.

## Development

This project is spec-driven with [OpenSpec](https://github.com/Fission-AI/OpenSpec).
Every non-trivial change starts as a proposal:

```bash
# propose a change
openspec list          # see active changes
# or in Claude Code:
/opsx:propose "your idea"
```

Project context, invariants and conventions live in
[`openspec/config.yaml`](openspec/config.yaml); agent workflow in
[`CLAUDE.md`](CLAUDE.md).

## Status

MVP, in progress. The `vyajena.v1` wire contract and the shared `protocol`
crate are in place (OpenSpec change `core-wire-protocol`), the server runs
locally (`server-core`), and the device core (`client/`, `client-core`) is
a Rust library that signs up, links devices, runs the MLS group and sends and
receives text, tested end to end against the real server. The Flutter app
(`app/`, `bridge/`, change `flutter-app`) runs on the iOS Simulator and the
Android emulator against a local server: join from an invite, automatic add of
invited users, chat with send status and notices (renames, members joining,
devices added or removed, this device removed), profiles, members, linking
a device by text, and the admin screens. It reconnects as soon as the
phone's network comes back. The server runs on the real server
(`deploy/server/`, change `server-deployment`), reached from anywhere through the GCP
relay (`deploy/relay/`, change `relay`). A closed app gets a content-free
"New message" notification through FCM or APNs (change `push`). Release builds are ready for Play internal testing and TestFlight
(`app-release`).

Next, in order: question-and-answer verification
(`qa-verification`), then the terminal client.
Desktop builds come later. See the open decisions in the architecture doc.

## License

GNU Affero General Public License v3.0 (`LICENSE`), like the original vyajena
(now `koppathachan/vyajena-legacy`). Running a modified server for others
means offering them its source.
