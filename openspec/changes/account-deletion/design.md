## Context

- **Admin-side removal already exists.** `Admin.RemoveUser` revokes every device of a
  user and their unused invites and frees the username (group-membership: Removing a
  user); `replace-user` extends it to tell the removed user's own devices in-band. A
  self-deletion is the same removal, triggered by the user instead of the admin.
- **MLS eviction already has a server-driven path.** The server, as external sender,
  issues external-Remove proposals for a device and any member device may commit them;
  admin devices commit pending ones on next connect (group-membership: Inactive devices
  are kept out). This is the "server marks removed → committed on next connect" pattern
  the self-delete reuses, so no new commit authority is introduced.
- **A device can already wipe itself.** `reset()` returns a device to a clean
  no-account state and mints nothing (client-runtime: Device reset, change
  `replace-user`). The local wipe after deletion is exactly this.
- **A blob can already be purged on demand.** `Blobs.DeleteBlob` removes the one stored
  copy and is a safe no-op on an absent blob (blob-store, change `delete-messages`).
- **Identity is device keys in the OS key store; there is no password and no logout**
  (auth model). "Deleting the account" therefore means: the server forgets the user,
  and the device forgets its keys.
- **Why this change exists:** Apple 5.1.1(v) and Google Play both require in-app,
  user-initiated account deletion; "the admin removes you" and "email us" are rejected.
  It is public-release blocker **B2** (the submission notes); `report-and-block` is B3.

## Goals / Non-Goals

**Goals**
- A user can delete their own account and data from inside the app, irreversibly.
- The server deletion reuses the existing removal, close-DM, external-Remove and
  blob-purge machinery, triggered by the user.
- The admin cannot self-delete into a permanent lockout.
- The deleting device and the user's other devices end at a clean no-account state.

**Non-goals**
- A grace period / scheduled deletion (allowed by the stores, not added here —
  deletion is immediate).
- Data export before deletion; deleting another user (stays `Admin.RemoveUser`);
  admin succession/recovery (`admin-recovery`); report/block (`report-and-block`).

## Decisions

### D1. A self-service RPC on `Devices`, scoped to the caller

- **`Devices.DeleteAccount(DeleteAccountRequest) → DeleteAccountResponse`**,
  authenticated by the caller's session token like every non-bootstrap RPC
  (wire-transport: Session authentication). It lives in `Devices` beside
  `RevokeDevice` and `GetSelf` — self-service account operations — not in `Admin`
  (admin-only).
- **The request carries no `user_id`.** The server deletes the **authenticated
  caller's own user**. This removes any authorisation surface: the RPC cannot be
  pointed at another account, so there is no "delete anyone" power to guard, only "the
  admin may not delete itself" (D4). `DeleteAccountResponse` is empty.
- **Why not reuse `Admin.RemoveUser` directly:** that RPC is admin-authorised and takes
  a target user; a member calling it on themselves would need member authorisation and
  self-targeting, which is a different contract. A dedicated self-RPC keeps both rules
  obvious and lets the admin be refused in one place.

### D2. What is deleted, what is retained (state it plainly for the stores)

| Data | On self-delete | Why |
|------|----------------|-----|
| User's devices | **revoked** | reuse `Admin.RemoveUser`; revoked devices can't authenticate |
| User's identity key | **dropped** | the removed marker keeps no identity material |
| Unused invites for the username | **revoked** | reuse `Admin.RemoveUser` |
| Key packages | **deleted** | server-held, belong to the user |
| Push tokens | **deleted** | server-held, belong to the user |
| Queued / undelivered messages to the user | **deleted** | the user's own copies, deleted at their request |
| Blobs the user uploaded | **purged** (`Blobs.DeleteBlob` semantics) | one stored copy per `blob_id`, keyed by uploader |
| Group membership | **evicted** via external Removes (D3) | reuse the inactivity path |
| DMs | **closed**, peers told `DmClosed` | reuse `close_dms_of` |
| `users.removed_at` marker (+ username) | **retained, minimal** | free & re-issue the username; keep revoked devices revoked |
| Messages already delivered to other members | **not erased** | server holds no history; E2EE can't reach other devices |
| Local account, chats, history, key-store keys | **wiped** on the device (D5) | `reset()` |

- The confirmation dialog and `app/store/privacy.md` say the same in plain words:
  everything the service holds for you is deleted; copies already delivered to other
  people's devices stay on their devices (true of any messenger).

### D3. MLS eviction reuses the server external-Remove path, not a new self-commit

- On an authorised delete the server issues an **external-Remove proposal for every one
  of the user's devices in every group** they are in (lobby + named groups), exactly as
  the inactivity path does (group-membership: Inactive devices are kept out). A
  remaining member device — or the admin device — commits the pending Removes **on next
  connect**; the server accepts that commit from any member. The server also deletes the
  user's queued entries in those groups immediately.
- **Why not have the deleting device self-commit its own leaf Removes first?** It is
  allowed (a device may Remove its own user's devices, group-membership: Commit
  authority) and could be a best-effort optimisation when online, but it is **not
  required**: the deleting device may be the group's only connected member, or may go
  offline the instant it taps delete. The server-issued external Removes guarantee
  eviction regardless and are already the system's answer to "a member device that will
  not commit its own removal." Keeping one mechanism keeps the change minimal.
- **Durability invariant holds:** the only queued copies deleted are the deleting
  user's **own**; nothing another member accepted is dropped. Peers keep their delivered
  messages.

### D4. The admin cannot self-delete (the dangerous edge case)

- **The server rejects `Devices.DeleteAccount` from the admin** with
  `FAILED_PRECONDITION`, changing nothing. This is the critical safety call-out:
  - Bootstrap is possible **only on an empty-users database** (auth model;
    device-identity: Bootstrap admin), so a self-deleted admin could not simply
    re-bootstrap on a server that still has other users.
  - **Single-device admin key-loss is a permanent lockout** (auth model): a wipe of the
    admin's only device with no recovery leaves no way back.
  - **The lobby's only committer is the admin** (admin-recovery: Break-glass re-seeds
    the admin's lobby): removing the admin would strand the lobby with no one able to
    commit.
- **The admin's reset path stays the operator-only `server admin-reset`** on the host
  (admin-recovery: Operator break-glass reset; Reset needs genuine host access), which
  re-seeds the lobby and re-bootstraps a fresh admin identity. Routing admin deletion
  there — and **refusing it over the RPC** — upholds admin-recovery's hard rule that
  *no network-reachable or unauthenticated endpoint resets or re-seats the admin*.
- **The app does not even offer it to the admin** (app-ui: The admin sees no
  self-delete), using the admin-awareness flag it already has (device-identity: Admin
  awareness). Server refusal is the backstop; the UI guard avoids a confusing dead-end.

### D5. Local wipe reuses `reset()`, only after the server confirms

- On an **OK** `DeleteAccount` the client calls the existing `reset()` (client-runtime:
  Device reset): local account, groups, DMs, history, pending jobs and the device's
  identity/auth/MLS/storage keys from the OS key store are erased, and the app returns
  to onboarding. On failure the client wipes nothing and keeps the account.
- The user's **other** connected devices learn via the **account-removal notice**
  (device-identity: Account-removal notice, change `replace-user`) — the same in-band
  frame a `replace-user` removal delivers — and can reset themselves; an offline other
  device learns on its next `Authenticate` (`PERMISSION_DENIED` → `Removed`). This is
  pure reuse: no new frame is added by this change.

### D6. Additive wire; server-first deploy; pbgen is a local step

- The only wire addition is the `Devices.DeleteAccount` RPC and its two messages — all
  additive, so `buf breaking` passes. **No new protocol version** and no new
  `ServerFrame` variant (the notice is reused from `replace-user`).
- **`cargo run -p pbgen` is a local implementation step**, run after the proto change
  and committed with it; **CI does not run codegen** (CLAUDE.md). The task list calls it
  out so the generated `pb.rs` files are regenerated and committed, not hand-edited.
- **Server before app:** an older server has no `DeleteAccount`; the app gates the
  action on a server that advertises it (as other wire changes gate their actions), so
  the button appears only against a server that honours it.

### D7. `reset()` is added here; other devices learn via `PERMISSION_DENIED` (not a new frame)

This change was specced assuming `replace-user` had shipped two reusable pieces: a client
`reset()` device-wipe (client-runtime: Device reset) and an account-removal `ServerFrame`
notice (device-identity: Account-removal notice). **At implementation (2026-11-26)
neither existed on `main`.** `replace-user` was merged as `reinvite-user` (#37), which
took a deliberately proto-free, admin-driven re-key design (shared note
`reinvite-user-reintegrated.md`: "added NO proto fields and NO migrations"): it delivered
no `reset()` API and no account-removal frame. The earlier spec text ("reuse `reset()`",
"reuse the account-removal notice") was therefore wrong about the dependency. Per CLAUDE.md
rule 5 the spec is corrected here, owner/tracker approved:

- **Local wipe:** this change adds the `reset()` core API itself (delete the device's five
  key-store keys — identity, device-auth, MLS signature, storage, session token — wipe the
  local store, and return the core to `NotRegistered` so the app shows onboarding). It is
  the behaviour D5 and the "Device reset" requirement text already describe; only the
  *owner* of the code moved from `replace-user` to this change.
- **Other devices:** no new `ServerFrame` is added. The user's other devices are revoked
  by the same removal, so each one's next `Devices.Authenticate` returns `PERMISSION_DENIED`,
  which the core already surfaces as `Removed` (client-runtime: Connection lifecycle). That
  authoritative signal — the fallback the `replace-user` design itself called the reliable
  path (its D1) — replaces the best-effort in-band notice. The spec's "second device"
  scenario is reworded to it. This keeps the wire **purely additive**: the only addition is
  the `Devices.DeleteAccount` RPC and its two messages, so `buf breaking` stays clean and
  no protocol-version bump is needed.
- **UI gate:** keeping the wire to just the RPC means there is no `server_knows_…`
  capability flag for the app to read (those come from protocol-version negotiation, and
  no version is bumped). So the Profile action is shown and, against a server too old to
  implement the RPC, the call returns `UNIMPLEMENTED`, which the app surfaces as an error
  instead of a dead delete. The owner's single server is upgraded before the app ships
  the action (server-first deploy), so no reachable server is too old in practice. This
  is the one deviation from the earlier "advertises / hides it" wording, reworded in the
  app-ui spec.

### Flow

```mermaid
sequenceDiagram
    participant U as Bob's phone (deleting)
    participant T as Bob's tablet (other device)
    participant S as server
    participant M as a remaining member / admin
    U->>S: Devices.DeleteAccount {} (authenticated: Bob)
    S->>S: caller is Bob, not the admin? (else FAILED_PRECONDITION)
    S->>S: mark removed, revoke Bob's devices + invites
    S->>S: issue external Removes for Bob's devices in each group; drop his queue entries
    S->>S: close Bob's DMs (DmClosed to peers); delete key packages, push tokens, queued msgs; purge blobs
    S-->>T: account-removal notice (in-band), before revoke closes the stream
    S-->>U: OK
    U->>U: reset() — wipe keystore + local DB, go to onboarding
    Note over M: on next connect, commits the pending external Removes
    M->>S: COMMIT (Removes of Bob's devices)
    S-->>M: accepted — Bob is no longer a group member
```

## Store compliance mapping

| Requirement | How this change meets it |
|-------------|--------------------------|
| **Apple 5.1.1(v)** — account deletion initiated **from within the app**, not email/support | "Delete account" in Profile → `delete_account()` → `Devices.DeleteAccount` (app-ui: Deleting your account; account-deletion: User deletes their own account) |
| **Apple 5.1.1(v)** — a **full** delete, not just deactivation | Immediate, irreversible removal of the user record, devices, key packages, push tokens, queued messages and blobs (D2); no deactivated state |
| **Apple 5.1.1(v)** — optional grace period allowed | Not used; deletion is immediate (a grace period is a possible later change) |
| **Google Play account-deletion policy** — in-app request to delete **account and associated data** | Same in-app action deletes the account and the server-held associated data (D2); the app does not rely on a web URL |
| **Google Play Data safety** — deletion is accurately described | `app/store/privacy.md` and the Data-safety form are updated to state in-app deletion exists and what it deletes vs. retains (task 7) |
| **Both** — the user can delete **only their own** account | The RPC targets the authenticated caller; no target id (D1) |

## Spikes

- **None required.** Every server step is an existing, tested operation
  (`Admin.RemoveUser` removal, the inactivity external-Remove issuer, `close_dms_of`,
  `Blobs.DeleteBlob`) and the local wipe is the existing `reset()`. The work is wiring
  a self-service entry point and the UI, not new mechanism. If, at implementation, the
  per-group external-Remove issue for a deleting user turns out to differ from the
  inactivity issuer in a way not foreseen here, record it here before coding (CLAUDE.md
  rule 5).

## Risks / Trade-offs

- **A modified/offline client cannot force eviction instantly.** Eviction completes
  when a remaining member or the admin next connects and commits the external Removes —
  the same latency the inactivity path already has. The server revoke is immediate, so a
  deleted user's devices cannot act in the meantime even before the MLS commit lands.
- **Group with no other connected member.** If the deleting user was the only connected
  device, the Removes sit pending until someone connects; acceptable and identical to
  inactivity. In the **lobby**, the admin is the committer; if the admin is offline the
  Remove waits for the admin — but the account/devices are already revoked.
- **Peers keep delivered messages.** E2EE cannot erase other devices' copies; the UI and
  privacy doc say so, so the user is not misled about what "delete" reaches.
- **Admin refusal is a hard stop, not a flow.** A real admin who wants to leave must use
  the operator host path; the UI explains this rather than offering a dead-end button
  (D4).

## Migration Plan

1. Deploy the server with `Devices.DeleteAccount` (and the `replace-user` account-removal
   notice it reuses). Old apps keep working; they simply do not show the action.
2. Release the app (owner step): the Profile "Delete account" action, gated on a server
   that advertises the RPC.
3. Update `app/store/privacy.md` and the stores' data-deletion answers to state that
   in-app deletion exists (this clears blocker B2).

**Rollback:** rolling the server back to a build without `DeleteAccount` makes the RPC
`UNIMPLEMENTED`; the app's gate hides the action again. No data migration either way.

## Open Questions (owner) — RESOLVED 2026-11-26

1. **Grace period — RESOLVED: immediate.** Deletion is immediate with **no
   cancellable window** or deactivated state (both stores allow a grace period; the
   owner chose not to add one). The spec states immediate throughout (D2; proposal
   "What Changes"; app-ui: Deleting your account).
2. **Admin-disabled copy — RESOLVED: short, honest note.** The admin's "Delete
   account" entry is shown **greyed-out** with the note: *"The admin account can't be
   deleted from the app. To reset it, run `server admin-reset` on the server host (see
   admin-recovery)."* (D4; app-ui: The admin sees no self-delete.)
3. **Confirmation friction — RESOLVED: type-to-confirm.** Because the action is
   irreversible, the confirmation requires the user to **type their own username**;
   the confirm control stays disabled until the typed text matches, so no single tap
   can delete the account (app-ui: Deleting your account).
