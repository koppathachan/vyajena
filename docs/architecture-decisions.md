# Chat App MVP — Architecture Decisions

Nov 7, 2026 · updated Nov 8, 2026 (core-wire-protocol), Nov 12, 2026 (flutter-app), Nov 15, 2026 (relay) · @Akhil Sasidharan

## Scope

The MVP is old-school text messaging in one admin-run group, end-to-end encrypted with OpenMLS. It replaces Vyajena, which was a learning experiment.

**In**

- Text messages only
- One group, created and managed by the admin (Akhil)
- Several devices per user, each its own MLS member
- Invite-only signup
- Store-and-forward through the Pi server
- One delivery status: accepted by the server (⏳ sending → ✓ accepted)
- Push notifications as content-free wake pings (FCM, APNs), built in the change after the relay (owner, 2026-11-15)
- iOS and Android apps first; desktop later (decision #3)
- Material 3 UI, `ColorScheme.fromSeed` with a muted seed and the `neutral` variant

**Out**

- DMs, more groups, user-created groups
- Read receipts, presence, typing indicators
- Media, voice, video
- Server-side message history
- Encryption at rest (no SQLCipher)

## Architecture

Four components. Keys never leave the Rust core on each device. Decrypted text
reaches the Flutter UI only to be shown: Dart never stores, logs or sends it
(owner-approved wording, 2026-11-11: a UI that shows messages cannot keep the
old "plaintext never leaves the core" literally). The relay and the Pi see
ciphertext only.

```mermaid
flowchart LR
  subgraph device["Member device — plaintext lives only here"]
    ui["Flutter UI<br/>screens only, no network<br/>shows text, never stores or logs it"]
    core["Rust core<br/>OpenMLS · networking"]
    db[("SQLite<br/>messages, incl. own sent plaintext")]
    ks[("Keystore / Keychain<br/>device key · MLS keys")]
    ui <-->|"flutter_rust_bridge"| core
    core --- db
    core --- ks
  end

  subgraph gcp["GCP e2-micro, us-east1 — sees TLS bytes only"]
    relay["rathole relay<br/>TCP passthrough, no state"]
  end

  subgraph home["Home network, India — sees ciphertext only"]
    pi["Pi server<br/>tokio + tonic, one binary"]
    pidb[("SQLite WAL<br/>users · devices · KeyPackages · invites<br/>public group state · commit log · queues")]
    pi --- pidb
  end

  push["FCM / APNs"]

  core -->|"gRPC · HTTP/2 · TLS on 443"| relay
  pi -->|"dials out: rathole token + Noise"| relay
  relay ==>|"client bytes through the tunnel"| pi
  core -. "TLS session ends on the Pi, key pinned" .- pi
  pi -->|"content-free wake ping"| push
  push -.->|"wake"| core
```

- **Flutter UI:** draws screens only; never opens a connection. iOS and Android (`app/`); every call goes through `flutter_rust_bridge` to the core (`bridge/`, no logic). Decrypted text is in Dart memory only while shown; the core's SQLite keeps the only copy. The app watches whether the phone has a network (`connectivity_plus`, state only, no connection) and tells the core to reconnect at once when one comes up (qa-fixes).
- **Rust core:** OpenMLS, networking, local SQLite, keys in the OS key store (Keystore/Keychain on phones, the OS keychain on desktop). Linked through `flutter_rust_bridge`.
- **GCP relay:** dumb TCP passthrough so no port opens at home.
- **Pi server:** routes, orders and queues ciphertext; dispatches wake pings.

## Encryption

OpenMLS (RFC 9420) runs inside the Rust core on every device, so the relay and the Pi see only ciphertext.

| Party | Sees | Cannot see |
| --- | --- | --- |
| GCP relay | TLS bytes, client IPs, timing | Anything inside TLS |
| Pi server | Group id, member devices and their public leaf keys, admin id, epoch, sender, time, size, membership changes | Message content, group name and settings, profiles |
| Apple / Google | When a device gets a wake ping | Content, sender |

Each layer of a message on the wire, and who can open it:

```mermaid
flowchart TB
  subgraph tcp["TCP: relay"]
    subgraph tls["TLS 1.3: Pi"]
      subgraph env["Envelope: Pi reads metadata"]
        subgraph mls["MLS: members only"]
          pt["Text message, plaintext"]
        end
      end
    end
  end
```

- **Epoch rule (strict):** the Pi rejects any message tagged with an old epoch; the client re-encrypts and resends.
- **Key pruning:** OpenMLS default `max_past_epochs = 0` is kept; the strict rule makes it safe.
- **Sent messages:** the sender cannot decrypt its own ciphertext, so the client stores its own plaintext locally on send.
- **Devices:** every MLS member is a device. Each user has one Ed25519 identity key, held by their first device, which signs a certificate for each device; the certificate is the device's MLS credential. Losing the first device means an admin-driven reset (remove the user, new invite).
- **Credentials:** OpenMLS does not check credentials, so the core verifies every new or changed leaf's certificate against the user's pinned identity key, and that the certificate's MLS key equals the leaf's signature key.
- **Handshakes in the clear:** Commits and Proposals are MLS `PublicMessage`, so the Pi can check them with OpenMLS `PublicGroup` (signature, sender, commit rule) without any group secret. Application messages stay `PrivateMessage`.
- **Commit rule:** admin devices may commit anything. Other devices may commit only their own leaf update, adding or removing their own user's devices, and the Pi's inactivity Removes. The Pi and every client check the same rule.
- **Group metadata:** the admin's user id is in the group context, which the Pi can read (it enforces it anyway). Group name and settings travel only as encrypted `GroupSettings` messages, from admin devices. After any commit that adds devices, admin devices send the current name again and every device sends its own user's `Profile` again, so a device added later learns both; an equal value changes nothing on screen (qa-fixes).
- **Names:** the admin picks each username at invite time; it is the local part of `user@<server>` and users cannot change it. Display name and "about" travel as an encrypted `Profile`, set only by that user's devices. When a user's identity key changes (a reset), members drop the stored profile, so the new identity shows the username until it sets one.
- **Residual risk:** the Pi could hand out a fake identity key and KeyPackage the first time a client sees a user (keys are pinned on first sight) and read new messages. Only question-and-answer verification (decision #2, its own `qa-verification` change) stops this; until then the core pins keys and warns when one changes.
- **Residual risk:** the Pi cannot check membership tags, confirmation tags or encrypted path secrets. A dishonest member with a valid leaf can get a commit sequenced that honest clients reject, which stalls the group. Any MLS delivery service has this gap; there is no recovery flow yet.

## Server (Pi)

The Pi is the single authority for routing, ordering and membership, and never for message history. Rust, tokio + tonic, SQLite in WAL mode, one static binary.

- **Admin:** the admin's user id is in the Pi's config; the bootstrap invite creates that user, who creates the group with `Admin.CreateGroup` before inviting anyone.
- **Invites:** admin creates single-use invite codes, each for a username; signup = code + user identity key + device certificate.
- **Auth:** each device holds an Ed25519 auth key and signs a server challenge, bound to the TLS connection, for a session token of at most 1 hour. A revoked device with a valid signature gets `PERMISSION_DENIED` (its open stream is closed the same way); the core then stops connecting and the app says the device was removed (qa-fixes). A bad signature stays `UNAUTHENTICATED`, so nobody learns whether a device is revoked without its key.
- **Key directory:** stores and hands out each device's MLS KeyPackages with its certificate.
- **Membership:** tracks the group's public state with OpenMLS `PublicGroup`, checks every Commit, and as an MLS external sender proposes the removal of inactive devices.
- **New users join on their own (flutter-app, owner 2026-11-12):** the admin inviting a user is the consent, so nobody taps "add". Once a new user's device uploads its first KeyPackages, the Pi sends a `JoinNotice` to the admin's connected devices, and again on every admin connect until that device is a member. The first admin device fetches that device's KeyPackage, checks its certificate (pinning the user's key on first sight) and commits the Add with its Welcome; a race loser gets `STALE_EPOCH` and does nothing. A changed identity key waits for the admin to accept it, and the accept is the add.
- **Ordering:** checks the epoch on every envelope, gives each accepted one the next `server_seq` for the group, and appends Commits and Proposals to the commit log.
- **Queues:** per-device store-and-forward queue of application ciphertext.
- **Push:** sends a wake ping to FCM or APNs when a device with no open stream gets a new entry, at most once a minute. Real FCM / APNs come in the change after the relay (owner, 2026-11-15); until then the app registers the core's placeholder token.

**What it stores:** users and their identity public keys, devices with their certificates, KeyPackages, invite codes, push tokens, session token hashes, the group id + epoch + member devices + public ratchet tree and group context, the commit log with a cursor per device, pending ciphertext, and dedup records (sender device, `msg_id`) for 7 days. Nothing else.

## Delivery and limits

Messages wait on the Pi at most 7 days and are deleted as soon as the recipient device acknowledges them.

Life of one application message on the Pi (Commits, Proposals and Welcomes skip the cap and expiry):

```mermaid
stateDiagram-v2
  [*] --> Received: device sends envelope
  Received --> Rejected: old epoch, too large, malformed or duplicate mismatch
  Received --> Accepted: durably written, sender shows accepted
  Accepted --> Queued: one entry per active recipient device
  Queued --> Delivered: device online, or woken by push
  Delivered --> Queued: disconnects before acking
  Delivered --> Deleted: device acks
  Queued --> Dropped: device inactive, 7 days offline or queue at 1,000
  Queued --> Expired: connected 7 days without an ack
  Rejected --> [*]: sender is told
  Deleted --> [*]
  Dropped --> [*]: device removed, re-added on return
  Expired --> [*]: device shows messages before X expired
```

| Rule | Value |
| --- | --- |
| Message expiry | 7 days, a backstop for a device that connects but never acks |
| Queue cap | 1,000 application messages per recipient device |
| Inactivity | A device offline 7 days, or whose queue reaches 1,000, is removed and re-added automatically when it returns |
| Max message size | 64 KB of application ciphertext |
| Handshake size | 256 KB per Commit or Proposal envelope, a Commit's Welcome included |
| Commits, Proposals and Welcomes | Exempt from expiry and cap; deleted once every device that must receive them has acknowledged |

- **Accepted** = the Pi has durably written the envelope and all its queue and log entries before it acknowledges.
- **Idempotent sends:** each envelope carries a client-generated id; the Pi deduplicates on (sender device, id) for 7 days, so retries never create duplicates.
- **Never blocked by one device:** a send is never rejected because of another device's queue. The full device is marked inactive instead.
- **Inactive devices:** the Pi stops queueing for the device, deletes its queue and sends an MLS external Remove proposal, reissued each epoch until committed. When the device returns, the Pi tells it so and sends a rejoin notice to the user's other devices and the admin's devices; the first one online re-adds it in one commit with no user action. The app shows "messages since X were not kept".
- **Catch-up:** a returning device gets everything after its cursor, commits, Welcomes and queued messages merged in `server_seq` order, before live traffic. (Replaying all commits first would leave older-epoch messages undecryptable, since `max_past_epochs = 0`.)
- **Expired gap:** messages that expired before a connected device acked them are lost; the app shows "messages before X expired".

## Connectivity

No port is opened at home: the Pi dials out to a free GCP relay, and TLS ends on the Pi, not the relay.

```mermaid
sequenceDiagram
  participant C as Rust core (phone)
  participant R as GCP relay (rathole)
  participant P as Pi server
  P->>R: Dial out, authenticate with rathole token + Noise
  Note over R,P: Tunnel stays up. Nothing listens at home.
  C->>R: TCP connect to port 443
  R->>P: Forward the connection through the tunnel
  C->>P: TLS 1.3 handshake, relayed byte for byte
  Note over C,P: Client compares the Pi's key with its pin
  C->>P: Device signs the Pi's challenge
  C->>P: gRPC over HTTP/2, Messaging.Connect stream
```

- **Relay:** its own GCP free-tier e2-micro, `vyajena-relay` in `us-east1-b` (project `vyajena`, its own VPC), running rathole 0.5.0 as a TCP passthrough: 443 for devices, 2333 for the tunnel. A reserved static IPv4 named `vyajarelay.purplechaamba.com` (one A record; owner, 2026-11-16): invites name `vyajarelay.purplechaamba.com:443`, so moving the relay needs a new A record, not a re-join. The Pi's tunnel dials the IP and needs no DNS. It holds no state and can be rebuilt, or moved to any VPS, from `deploy/relay/` and `~/.vyajena-relay/` (change `relay`).
- **Tunnel auth:** rathole's Noise NK transport with the relay's static key pinned on the Pi (a fake relay cannot attach the Pi), plus a service token (nobody else can attach in the Pi's place). The tunnel port is open to all because the home IPv4 is dynamic.
- **TLS:** the client pins the Pi's TLS public key. No domain or certificate authority needed.
- **Transport:** gRPC over HTTP/2 + TLS on TCP 443. HTTP/3 (tonic-h3) stays deferred while it is experimental.
- **Latency:** India → US relay → Pi in India: two intercontinental legs per round trip, roughly 0.4–0.6 s per send. Accepted for the MVP.
- **Cost:** the static IPv4 is billed, about $3.65/month (owner, 2026-11-15); the e2-micro, 10 GB standard disk and 1 GB/month outbound data are free tier. Watch the bill and the outbound data.
- **Direct IPv6** to the Pi (the Airtel router drops inbound IPv6 today) is a possible later improvement, last.

### Home-LAN phase (ended 2026-11-15)

From `pi-deployment` until the `relay` change, devices on the home Wi-Fi
dialed the Pi at `192.168.1.8:8443` and nothing outside home could reach it.
The relay change moved the server to `127.0.0.1:8443` behind the tunnel,
removed the LAN 8443 rule, and reset the Pi (no migration): every device
re-joined with an invite naming the relay.

## Storage, keys and hardening

Messages are not encrypted at rest; keys live in the OS key store and every machine is hardened by access control. The one exception is OpenMLS group state, sealed under a storage key from the key store (client-core).

**App**

- Plain SQLite for messages, relying on Android FBE / iOS Data Protection.
- Ed25519 device key and MLS private keys in Android Keystore / iOS Keychain, never in the DB and never in Dart. iOS: the data-protection keychain, `WhenUnlockedThisDeviceOnly`, not synced. Android: each secret sealed under a non-exportable Keystore AES key in app-private preferences (flutter-app D4).
- Android: the core's folder in `noBackupFilesDir`, `allowBackup=false`, and no cloud backup or device transfer. iOS: the folder in Application Support, excluded from iCloud backup, `NSFileProtectionComplete`.
- Accepted risk: a rooted or jailbroken phone can read messages.

**Desktop (terminal client; client-core)**

- The identity, device auth and MLS signature keys, and the storage key, live in the OS keychain: macOS Keychain, Linux Secret Service, Windows Credential Manager.
- Only on a machine without a keychain, and only after the user explicitly opts in, they live in one key file with mode `0600`; without the opt-in the core refuses to create keys and says how to opt in.
- OpenMLS group state (epoch secrets, leaf keys, secret tree) is stored in SQLite with every value sealed (XChaCha20-Poly1305) under the storage key, so a copied database shows messages but cannot decrypt new group traffic. Messages are plain SQLite, as on phones.

**Pi**

- SSH with keys only, no password or root login.
- nftables drops all inbound except SSH from the home subnet; the Pi only
  makes the outbound tunnel. The server listens on loopback only and may
  reach only loopback; the tunnel (`vyajena-tunnel.service`, a dynamic user)
  only loopback and the relay.
- Server runs as its own system user `vyajena` (no login shell); database and
  keys in `/var/lib/vyajena`, mode `700`, files `600`.
- systemd sandboxing: `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome`,
  `PrivateTmp`, no capabilities, a system call filter; writable only
  `/var/lib/vyajena`. It stops cleanly on `SIGTERM`.
- Unattended security updates, never an automatic reboot (the hardening check
  reports a pending one); nothing but SSH and the server listens (mDNS off).
- Built on the Mac in an arm64 container; no toolchain on the Pi. Deploy,
  check and backup are one command each (`deploy/pi/`).

**Relay**

- Its own VM and VPC (owner, 2026-11-15): nothing else shares it. Worst case for chat, if it is compromised: DoS plus client IPs and timing.
- VPC firewall: 443 and the tunnel port public, SSH only from Google IAP's range, everything else dropped; firewall logging off.
- SSH only through GCP IAP, keys only (the relay key in `~/.vyajena-relay/`), no root; the host key is pinned from the VM's guest attributes.
- VPC flow logs off, rathole logs at `warn`, the journal in RAM only: client IPs are not kept.
- No service account or API scopes; rathole runs under a dynamic user with only `CAP_NET_BIND_SERVICE` and no writable path.
- Unattended security updates, never an automatic reboot; checked by `deploy/relay/check.sh`.

## Wire protocol

One persistent bidirectional gRPC stream carries all messaging; `.proto` files are the contract, independent of the Rust code.

The contract is package `vyajena.v1` in `proto/vyajena/v1/`.

| Service | RPCs |
| --- | --- |
| `Messaging` | `Connect` (bidi stream): protocol version hello, envelopes both ways, send results, acks, catch-up, gap / removed / rejoin / join / low-KeyPackage notices (`JoinNotice`: a new user's device waits for the admin's device to add it), heartbeats |
| `Devices` | `GetServerInfo`, `Register` (invite), `GetChallenge`, `Authenticate`, `LinkDevice`, `GetSelf`, `RevokeDevice`, `UpdatePushToken` |
| `Keys` | `UploadKeyPackages` (a new device's first upload triggers the `JoinNotice`), `FetchKeyPackages` (one user, optionally only some devices), `GetIdentityKeys` (identity keys only, consumes nothing) |
| `Admin` | `CreateInvite`, `ListInvites`, `RevokeInvite`, `CreateGroup`, `RemoveUser` |

**Envelope:** `msg_id` (16 bytes, client-generated, for dedup) · `group_id` · `epoch` · `kind` (application, commit, proposal, welcome) · `ciphertext` (the MLS message) · `welcome` (on a commit only: Welcome bytes + recipient devices). Set by the Pi: `sender` · `server_seq` · `accepted_at`.

- **Welcome inside its commit:** clients never send `welcome` envelopes. A Welcome rides inside the commit that creates it; the Pi accepts or rejects both together, then sends each new device a `welcome` envelope itself.
- Server ids are `user@<server>` from day one, so federation can be added later. A device id is the first 16 bytes of SHA-256 of its auth public key.
- Every message passes through the Rust core; Flutter never opens a connection (checked by an import scan of `app/lib/`).

## Future

Nothing here is built in the MVP, but the protocol leaves room for each.

- DMs as 2-member MLS groups, id = hash of the sorted user pair
- More groups, then Discord-style servers; admin-only creation at first
- Media: client-encrypted blobs on the Pi, file key sent inside MLS
- P2P under conditions (media, voice, video) via iroh, Pi or relay as iroh-relay
- Encrypted per-user history store on the Pi, which also gives new devices history
- Federation between servers using `user@<server>` ids
- gRPC over HTTP/3 (tonic-h3) once it matures

## Open decisions

- [x] Multi-device in the MVP? **Yes** (core-wire-protocol): each device is an MLS member with a certificate signed by the user identity key; the first device holds that key and links the others; recovery is admin-driven.
- [x] Fingerprint verification in the MVP? **Replaced** (client-core): question-and-answer verification over a PAKE (CPace): both users type the answer to a question the asker picks, and the answer never leaves either device. It comes in its own `qa-verification` change right after the Flutter app (owner, 2026-11-11); the core already pins identity keys and warns when one changes.
- [x] Desktop clients in the MVP, or mobile only? **Mobile now, desktop later** (owner, 2026-11-11; flutter-app): the Flutter app targets iOS and Android only; desktop builds come in a later change.
- [x] Key rotation for idle groups? **Every 7 days, with jitter** (client-core): each device commits an update of its own leaf once it is 7 days old and replaces its last-resort KeyPackage once that is 7 days old, checking on connect and daily, with a random delay of up to an hour.
- [x] Max message size? **64 KB** of application ciphertext (core-wire-protocol); Commit envelopes, Welcome included, up to 256 KB.
- [x] Relay on the shared VM or its own VM? **Its own VM** (owner, 2026-11-15; relay): a new free-tier e2-micro in `us-east1`, project `vyajena` (owner, 2026-11-16; first built in `purple-eccdf`), used by nothing else; a static IPv4 (about $3.65/month) named `vyajarelay.purplechaamba.com`. Real push comes in the next change.
- [x] Build order? **Changed** (owner, 2026-11-11; flutter-app): 1. Pi server ✅, 2. Rust core library ✅, 3. Flutter app (iOS and Android; the first point where two people can chat locally, on two simulators), 4. question-and-answer verification (`qa-verification`), 5. relay and Pi deployment ✅ (relay first, real push (FCM/APNs) in the next change; owner, 2026-11-15), 6. the terminal client. (Was, pi-server-core: Pi, core, terminal client, deployment, then the app with push.)
- [x] How is the app distributed? **Privately first** (owner, 2026-11-15; app-release): a new Play app `com.purplechaamba.vyajena` on internal testing now, TestFlight internal testing once the owner's individual Apple account exists; Android 7.0+ and iOS 15.0+, iPhone only; name "Vyajena", icon "വ്യ"; export compliance `ITSAppUsesNonExemptEncryption = YES` (mass-market, France left out); upload key and certificate held only by the owner (Keychain + password manager), never in the repo or CI; policy at https://www.purplechaamba.com/vyajena/privacy/. The old vyajena Play app and its CI deployment are deleted once this app runs. A **public** listing waits for: the relay (reviewers must reach the server), in-app account deletion, report/block, real push, and Play's 12-tester closed test. Steps: `docs/release.md`.
