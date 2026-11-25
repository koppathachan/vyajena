# Design — admin-recovery (break-glass)

## Context

The admin is pinned by identity key and is the only authority on the server. A
single-device admin that loses its OS-key-store keys cannot authenticate, cannot
re-key, and cannot be re-invited by anyone, because:

- `ensure_bootstrap_invite` (`server/src/runtime/bootstrap.rs`) re-mints only on an
  empty `users` table (`if users > 0 { return NotNeeded }`).
- `Admin.CreateInvite` is admin-only, and the admin is gone.
- The removed-user re-key path exists (`server/src/devices/mod.rs:420` — on
  `Devices.Register` for a user whose `removed_at` is set, replace
  `identity_public_key` and clear `removed_at`, old devices stay revoked) but is only
  reachable through an invite, which only the admin can mint.

The owner's decision (`auth-model-and-admin-lockout`, 2026-11-25): the admin owns the
box, so **operator/host access is the recovery authority**. No cryptographic recovery
key is needed for this path. We reuse the two mechanisms that already exist —
bootstrap minting and the removed-user re-key — and add the one gate that connects
them (a removed admin re-fires bootstrap) plus the client trust handling the re-key
forces.

**The two things being recovered are different and must not be conflated:**

| Recovered thing | What it is | How it is regained (this change) |
|---|---|---|
| **Admin authority** | the right to call Admin RPCs | owning the server → `server admin-reset` on the box → re-bootstrap → re-register |
| **Peer E2EE trust** | members trusting the admin's key is really the admin's | the key-change acknowledgement ("re-trust?"), never silent auto-accept |

Regaining authority does **not** regain trust: the re-keyed admin is, to every peer, a
changed safety number. That is intended and is surfaced, not hidden. The richer trust
recovery (same-key recovery phrase, quorum vouches, badges) is the deferred
`trust-verification` change; here we ship only the bare acknowledgement.

## Flow

```mermaid
sequenceDiagram
    participant Op as Operator (host access)
    participant DB as Server DB (SQLite, 0600)
    participant Srv as Server process
    participant Adm as Admin's new device
    participant Mem as Member device

    Note over Adm: only device wiped — keys gone, locked out
    Op->>DB: server admin-reset --config (local, direct DB edit)
    DB-->>DB: users.removed_at = now for admin;<br/>revoke admin's devices + unused invites
    Op->>Srv: restart (or next start)
    Srv->>DB: ensure_bootstrap_invite: no ACTIVE admin?
    DB-->>Srv: admin removed, no usable bootstrap invite
    Srv-->>Op: re-mint ONE bootstrap invite, print once
    Op->>Adm: hand over bootstrap invite (out of band)
    Adm->>Srv: Devices.Register(invite, NEW identity key, cert)
    Srv->>DB: removed-user re-key: set new identity_public_key,<br/>clear removed_at; old devices stay revoked
    Srv-->>Adm: registered — admin authority restored
    Note over Mem: still has admin's OLD key pinned
    Mem->>Srv: process Add of admin's new device / lookup key
    Srv-->>Mem: admin's NEW identity key
    Mem-->>Mem: pin new key, mark admin UNVERIFIED,<br/>show "⚠️ safety number changed — re-trust?"
    Note over Mem: owner must acknowledge before admin is trusted
```

## Decisions

### D1. Recovery authority = host access, not a cryptographic recovery key.
The admin owns the self-hosted box. Anyone who can run `server admin-reset` can
already read and delete the whole database and key directory; re-seating the admin
adds no capability they do not already have. A cryptographic break-glass key stored
somewhere would be a second long-lived secret to lose or leak, reintroducing exactly
the single-point-of-failure this change removes. So the authority is simply filesystem
access to the server's data directory. Same-identity cryptographic recovery (keep the
key, no re-trust) is still worth having for the *common* case and is the deferred
`trust-verification` part A — but it is not needed to unbreak a locked-out admin.

### D2. The reset is a local subcommand that edits the DB directly, never a network endpoint.
`server admin-reset --config <file.toml>` joins the existing hand-rolled subcommands
(`backup`, `egress-proxy`) in `server/src/main.rs`. It `Config::load`s to find the
database path, opens that SQLite file (which is mode 0600, owned by the service user),
and in one transaction:

- sets `users.removed_at = now` for `config.admin_username`;
- revokes the admin's existing devices (so a stale/compromised old device cannot
  linger — consistent with `device-identity: Device revocation`);
- revokes the admin's unused invites.

It mints no invite and starts no listener. **Rejected: an unauthenticated recovery
RPC / HTTP endpoint.** A network-reachable reset is a catastrophic attack surface (a
single request re-seats the admin) and would contradict the whole trust model. The
only way in is already having the box. The command refuses to run if it cannot open
the configured database (no host access ⇒ no reset). Because it runs against the
server's own `Config`, it is a deployment/ops action, documented alongside
`server-deployment: Scripted reset` (which it is distinct from — see D4).

### D3. Optional operator passphrase (defense-in-depth, OQ1).
Host access is the authority, but shared boxes, forgotten sudo sessions and backups
make "filesystem access" fuzzier than "a secret the operator knows". So the command
MAY additionally require an operator passphrase, set in the server config
(`server-runtime: Configuration`, an optional field) or prompted interactively; when
set, a wrong/absent passphrase refuses the reset and changes nothing. This is a floor,
not the authority: with no passphrase configured, host access alone is sufficient (the
owner may not want another secret to lose — the very failure mode we are fixing). Left
to the owner (OQ1); the spec allows it as optional and does not require it.

### D4. `admin-reset` is surgical; it is NOT the whole-server `Scripted reset`.
`server-deployment: Scripted reset` wipes the entire server (new TLS pin, empty
database, every device re-joins). That is the wrong tool for a locked-out admin: it
throws away every other user, every group and the server identity. `admin-reset`
changes exactly one account — the admin's — and re-seeds the admin-owned system lobby
(D9), leaving the TLS pin, the server keys, every other user's account, every named
group and every DM untouched. Members keep their accounts and their DMs and named
groups; they rejoin the fresh lobby automatically and re-trust the admin's new key.
Two different operations; the spec keeps them separate.

### D5. Re-bootstrap gates on an ACTIVE admin, not an empty database.
The minimal change to make the removed admin recoverable: `ensure_bootstrap_invite`
stops gating on `users > 0` and instead gates on "an **active** admin exists" — the
admin user id is present AND `removed_at IS NULL`. Precisely:

- no active admin **and** no usable bootstrap invite ⇒ mint one (as today, printed
  once);
- active admin ⇒ `NotNeeded` (as today for a healthy server, regardless of other
  users).

This is a strict superset of today's behaviour: on a genuinely empty database there is
no admin row, so it still mints; once the admin registers, the admin is active and it
stops. It newly fires after `admin-reset` marks the admin removed. It never fires on a
healthy server whose admin is active, even with many other users. The re-minted invite
is single-use and 7-day-expiring like any bootstrap invite, so a stale un-used invite
does not linger as a backdoor. (Operator guidance, OQ2: `admin-reset` may print a line
reminding the operator that members will see a re-trust prompt, so they can warn them
out of band — no behaviour change.)

### D6. The admin re-keys; peers re-trust by acknowledgement, never silently.
Re-registration goes through the existing removed-user re-key path, so the recovered
admin has a **new** identity key. Every member still has the admin's **old** key
pinned. The baseline (`key-directory: Clients verify the identity chain`,
`group-membership`) already: looks up the changed key, verifies the new device cert
against it, pins the new key and *warns*. This change tightens "warn" into
**acknowledge-before-trust**:

- the client still pins the new key (it must, to keep decrypting the group) **but**
  marks that identity **unverified / pending re-trust**;
- it surfaces a clear prompt — "⚠️ <admin>'s safety number changed — re-trust?" — and
  SHALL NOT present that identity as trusted/verified until the device owner
  acknowledges;
- it SHALL NEVER silently auto-accept the changed key as verified.

This is the impersonation protection: a hostile or coerced server that swaps in an
attacker's key as "the admin" cannot get it silently trusted; every member sees the
change and must act. Pinning-to-decrypt and trusting-as-verified are deliberately
separated so the group keeps working while trust is withheld pending the owner's tap.
This bare acknowledgement is the **minimum** and intended handling for break-glass;
the quorum/badge system that makes re-trust nicer is deferred (`trust-verification`).

### D7. No proto change.
The reset edits the server's own DB; re-onboarding reuses `Devices.Register`
unchanged; the re-trust gate is a local client rule over already-specified
`Keys.GetIdentityKeys`/commit processing (the client already has
`Client::accept_identity_key` and a pending-key slot, `client/src/client.rs:872`, which
the acknowledge-before-trust gate builds on). Nothing crosses a new wire, so there is no
`.proto` edit and no `PROTOCOL_VERSION` bump.

### D8. Two devices do not cover identity-device loss — why break-glass is still the net.
The identity **private** key lives on **one** device only — the original/bootstrap
device. A linked second device never holds it: `link_request` deletes any leftover
identity key on the new device ("a leftover identity key would make this device act as
the user's identity device", `client/src/client.rs:459`), `approve_link` can only be
done by the device that holds the identity key (`identity_key(...).ok_or(NotIdentityDevice)`,
`:483`), and `complete_link` only **pins the identity public key** on the linked device
(`pin_identity_key`, `:619`). So a linked device:

- **can** do admin RPCs (create invites, remove users) and keep messaging; but
- **cannot** approve or link further devices, and **cannot** re-key or restore the
  identity-device role.

Consequences for recovery:

- **Losing a linked device** is fully recoverable from the original: re-link a
  replacement. No break-glass needed.
- **Losing the original/identity device** (keys gone) leaves a linked device still
  working and still admin-capable, but with **no way to add devices or recreate the
  identity device** — the user is stuck single-device with a non-extensible, non-re-keyable
  account, and if that was the only device, fully locked out. **Having two devices does
  NOT cover identity-device loss.** This is exactly where the operator reset → re-key
  break-glass is the net, and why same-key off-device backup of the identity key
  (deferred `trust-verification` Part A) is the real fix for the single point of failure.

### D9. Break-glass re-seeds the admin's lobby (owner decision, 2026-11-25).
The first cut reset the admin's *account* but left the system lobby's MLS state intact,
and that is not enough: the lobby's only committer is the admin, so a recovered admin's
new device had no way back in. Its new leaf was never added (the old revoked admin
device still held the only commit authority, and the stale old leaf plus members blocked
a fresh group), leaving the new device stuck "waiting to be added" while the lobby still
listed the old devices. The gap is a spec gap too: nothing said **who re-adds the admin
when no higher admin exists**. The owner's answer (2026-11-25): the break-glass must also
reset the server's group record for the admin's lobby, so the re-keyed admin re-seeds it
from scratch.

So `admin-reset` now, in the same transaction, drops the lobby's server state — queued
envelopes + deliveries, OpenMLS public state, member rows, pending server Removes, gap
notices — and deletes the `mls_groups` row (it cannot merely `closed_at` it: the system
`kind` forbids that, and `exists`/`main_row` ignore `closed_at`, so the row must go for
`CreateGroup` to run). It clears `was_member` on every former member device, so the
server re-announces each non-revoked member as waiting (the existing automatic-add path)
and the recovered admin re-adds it with a fresh Welcome on the member's unchanged
identity key; the member's client rejoins via the normal `on_welcome` path and, seeing
the admin's new key, shows the re-trust prompt (D6). The deletes reuse the exact set and
order of `dms::drop_group_state` (deliveries before envelopes; the `mls_groups` children
before the row), so the foreign keys stay satisfied.

**Flush the waiting announce at CreateGroup (test-selfhost race, 2026-11-25).** The
server only announces waiting devices to an admin device *when that device connects*.
The recovered admin connects before it creates the lobby, so the connect-time announce
fires with nothing waiting and is missed — the re-seeded members then sit "waiting"
until the admin app is force-relaunched (test-selfhost found bob and carol stuck this
way). Owner fix: `Admin.CreateGroup` now, right after seeding the lobby, re-sends the
waiting notices to the admin's member devices (`joins::waiting_notices`, the same
`joins::waiting` machinery the connect path uses), so the re-announced members are
re-added without a manual reconnect. It is idempotent — `joins::waiting` excludes any
device already in the lobby, so an ordinary first-ever `CreateGroup` (no waiting
members) sends nothing and an already-added member is never re-notified.

**History trade-off (accepted).** The recovered admin's new device loses pre-reset lobby
history — but that history lived in the lost identity device's key material and is
unrecoverable regardless, so nothing recoverable is thrown away. Members keep their own
local history; only the server-queued, now-undeliverable old-epoch traffic is dropped.
This is the right trade for a break-glass. DMs and named groups are deliberately left
alone (they are not admin-seeded lobbies); the admin's re-participation in those is out
of scope here and noted as a follow-up.

## Security notes

- **Authority stays on the box.** The only way to re-seat the admin is to run a local
  command against the server's own 0600 database. No request from the network can do
  it. This matches the deployment's existing trust root (the operator owns the host).
- **Re-key forces re-trust.** An attacker who re-seats the admin (which already
  requires owning the box) still cannot silently impersonate the old admin to members:
  the new key is a changed safety number that every member must acknowledge. The reset
  therefore cannot be used to quietly take over existing E2EE conversations.
- **Old devices stay revoked.** The admin's pre-reset devices are revoked by the
  command and by the removed-user re-key path, so a lost/compromised old admin device
  gains nothing from the recovery.
- **Blast radius is one account plus the lobby.** Unlike the whole-server scripted
  reset, `admin-reset` cannot be used to wipe other users' accounts, their DMs or named
  groups, or the server identity. It resets the admin account and re-seeds the shared
  lobby (D9); members rejoin automatically and keep their accounts, DMs and named groups.

## Alternatives rejected

- **Unauthenticated network recovery endpoint** — rejected (D2): a single request
  would re-seat the admin; unacceptable attack surface.
- **A stored cryptographic break-glass key** — rejected for this path (D1): another
  long-lived secret to lose/leak, reintroducing the single point of failure. Deferred
  same-key recovery phrase (`trust-verification` A) covers the key-preserving case
  instead, where it belongs.
- **Whole-server `deploy.sh --reset`** — rejected (D4): destroys every other user,
  group and the server identity to recover one admin.
- **Silent auto-accept of the admin's new key** — rejected (D6): breaks E2EE
  impersonation protection.

## Open questions

- OQ1: Require an operator passphrase on top of host access? (D3; default: optional.)
- OQ2: Should `admin-reset` print an operator reminder about the members' re-trust
  prompt? (D5; default: yes, guidance only.)
