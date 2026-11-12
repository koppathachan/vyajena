# vyajena2

End-to-end encrypted, old-school text messaging for one admin-run group.
Successor to the original `vyajena` learning experiment.

The MVP is invite-only and text-only, end-to-end encrypted with
[OpenMLS](https://openmls.tech/) (RFC 9420), and store-and-forward through a
self-hosted Raspberry Pi server. Keys never leave the Rust core on each device;
decrypted text reaches the Flutter UI only to be shown, never stored, logged or
sent from there. The relay and the server see only ciphertext.

## Architecture

Four components:

- **Flutter UI** — draws screens only (iOS and Android); never opens a
  connection, never stores or logs message text.
- **Rust core** — OpenMLS, networking, local SQLite, keys in the OS key store
  (Keystore/Keychain on phones, the OS keychain on desktop). Linked through
  `flutter_rust_bridge`.
- **GCP relay** — dumb TCP passthrough so no port opens at home.
- **Pi server** — routes, orders and queues ciphertext; dispatches wake pings.

Full rationale: [`docs/architecture-decisions.md`](docs/architecture-decisions.md).

## Run the server locally

Needs Rust (stable). Nothing runs as root; data stays in `server/.pi-local/`
(git-ignored).

```bash
cargo run -p server -- --config server/local.toml
```

```
first start ──> creates keys + database in server/.pi-local/
            ──> logs the TLS pin          (every start, never changes)
            ──> logs the bootstrap invite (ONCE, first start only)
            ──> listening on 127.0.0.1:8443 (TLS 1.3, HTTP/2)
```

- **TLS pin:** the log line `TLS key pin` → field `tls_spki_sha256`. Same on
  every restart.
- **Bootstrap invite:** the log line `bootstrap admin invite created` → fields
  `invite_code` and `payload` (what a client needs: relay address, pin, code).
  It creates the admin, `akhil@pi`, and is valid for 7 days.
- **Copy it the first time.** Later starts only say an invite is pending, never
  the code again.
- **Lost it, or want a clean slate?** Stop the server, then:

  ```bash
  rm -rf server/.pi-local
  ```

  The next start makes new keys (a new pin) and a new invite.
- **Settings:** `server/local.toml` (port, admin username, paths). An unknown
  or missing field stops the server with the field's name.
- **Stop:** Ctrl-C.

Run the tests (each one starts its own server in a temp folder, so they never
touch `server/.pi-local/`):

```bash
cargo test --workspace
```

## Run the app locally

Two phones on one Mac: an iOS Simulator and an Android emulator chatting
through your local Pi. Needs Flutter 3.44, Xcode with an iOS Simulator, the
Android SDK with an emulator, and the Rust phone targets (cargokit builds
`bridge/` from Xcode and Gradle):

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios \
  aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
```

```
1. local Pi        cargo run -p server -- --config server/local.toml
                   copy the bootstrap invite: log line "bootstrap admin
                   invite created", field payload (first start only)
2. iOS Simulator   open -a Simulator
                   cd app && flutter run -d <simulator id>
3. Android         flutter emulators --launch <emulator id>
                   adb reverse tcp:8443 tcp:8443
                   cd app && flutter run -d emulator-5554
4. first phone     paste the bootstrap invite → Check invite → Join
                   → Create the group                   (you are akhil@pi)
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
  later runs reuse that phone. A clean slate (`rm -rf server/.pi-local`)
  deletes the local Pi's users and group.

App tests never touch your local Pi or its invite:

```bash
cd app && flutter analyze && flutter test        # widget tests, a fake core
app/integration_test/run.sh <device id>          # the real app on a simulator
```

`run.sh` starts its own temporary Pi on a free port in a temp folder (never
`server/local.toml`), plays the admin's device on the Mac through the core
(`bridge/tests/app_peer.rs`), and runs `app/integration_test/` on the device
as Bob: join, added automatically, "hi" ⏳ → ✓, "hey" received. It stops the
Pi and removes the folder when done.

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
crate are in place (OpenSpec change `core-wire-protocol`), the Pi server runs
locally (`pi-server-core`), and the device core (`client/`, `client-core`) is
a Rust library that signs up, links devices, runs the MLS group and sends and
receives text, tested end to end against the real server. The Flutter app
(`app/`, `bridge/`, change `flutter-app`) runs on the iOS Simulator and the
Android emulator against a local Pi: join from an invite, automatic add of
invited users, chat with send status and notices, profiles, members, linking
a device by text, and the admin screens. Push is a placeholder until
deployment.

Next, in order: question-and-answer verification (`qa-verification`), relay
and Pi deployment with real push (FCM / APNs), then the terminal client.
Desktop builds come later. See the open decisions in the architecture doc.
