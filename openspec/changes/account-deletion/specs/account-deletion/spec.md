## ADDED Requirements

### Requirement: User deletes their own account
A user SHALL be able to delete their own account by calling the authenticated
`Devices.DeleteAccount`, which targets **the caller's own user** — the server
reads the user from the caller's session token and `DeleteAccountRequest` carries no
other user id, so the RPC can never delete another user. On an authorised call the
server SHALL, in one transaction, mark the user removed (`users.removed_at`), revoke
every device of that user, and revoke the user's unused invites, reusing the
`Admin.RemoveUser` removal path (group-membership: Removing a user) with the user as
the trigger instead of the admin. A revoked device SHALL receive no further
envelopes, and the username SHALL be free for a new invite. The server enforces the
authorisation and the removal.

#### Scenario: Member deletes their own account
- **WHEN** Bob's device calls `Devices.DeleteAccount` with a valid session token and
  Bob is not the admin
- **THEN** the server marks Bob removed, revokes all of Bob's devices and unused
  invites, and Bob's devices fail their next `Devices.Authenticate` with
  `PERMISSION_DENIED`

#### Scenario: Deletion cannot target another user
- **WHEN** Bob's device calls `Devices.DeleteAccount`
- **THEN** the server deletes only Bob's own account, because the request carries no
  target user id and the server uses the authenticated caller's user

#### Scenario: Unauthenticated delete is rejected
- **WHEN** a caller sends `Devices.DeleteAccount` without a valid session token
- **THEN** the server rejects it with `UNAUTHENTICATED` and deletes nothing

### Requirement: The admin cannot self-delete through the RPC
The server SHALL reject `Devices.DeleteAccount` when the caller's user is the admin,
with `FAILED_PRECONDITION`, and SHALL delete nothing. In-app account deletion is for
ordinary users; the admin's only reset path is the operator-only, host-gated `server
admin-reset` (admin-recovery: Reset needs genuine host access), because bootstrap is
possible only on an empty-users database, single-device admin key-loss is a permanent
lockout, and the lobby's only committer is the admin. This upholds admin-recovery's
rule that no network-reachable endpoint resets the admin. The server enforces it, and
the client refuses to call the RPC for an admin device (app-ui: Deleting your
account).

#### Scenario: Admin's delete is refused
- **WHEN** the admin's device calls `Devices.DeleteAccount`
- **THEN** the server rejects it with `FAILED_PRECONDITION`, leaving the admin
  account, its devices and the lobby unchanged

#### Scenario: No network path resets the admin
- **WHEN** any client, as the admin, asks the server to delete or reset the admin
  account
- **THEN** the server offers no such path; the admin is reset only by the local
  `server admin-reset` subcommand on the host

### Requirement: Deletion evicts the user from every group and closes their DMs
On an authorised `Devices.DeleteAccount` the server SHALL evict the deleted user from
every group they belong to — the lobby and every named group — by issuing server
external-Remove proposals for all of that user's devices, reusing the inactivity
external-Remove path (group-membership: Inactive devices are kept out): the proposals
are committed by a remaining member or the admin device when it next connects, and the
server SHALL delete the user's queued entries in those groups at once. The server SHALL
also close every DM the user is in, delivering `DmClosed` to each peer's devices
(reuse `close_dms_of`). The server enforces this; MLS eviction completes through the
normal commit-on-next-connect flow, with no new commit authority.

#### Scenario: Evicted from the lobby and a named group
- **WHEN** Bob, a member of the lobby and the named group "Home chat", deletes his
  account
- **THEN** the server issues external-Remove proposals for all of Bob's devices in
  both groups and deletes Bob's queued entries, and the next member or admin device to
  connect commits those Removes, after which Bob is no member of either group

#### Scenario: DM peers are told
- **WHEN** Bob has a DM with Alice and deletes his account
- **THEN** the server closes the DM and delivers `DmClosed` to Alice's devices, so
  Alice's app shows the DM as closed

#### Scenario: Deleting user's own undelivered copies go
- **WHEN** Bob deletes his account while messages addressed to Bob's devices are still
  queued and undelivered
- **THEN** the server deletes Bob's queued entries as part of the removal, because Bob
  asked to delete his own account

### Requirement: What the server deletes and what it retains
On an authorised `Devices.DeleteAccount` the server SHALL delete the account data it
holds for the user: the user's uploaded key packages, the user's push tokens, and the
user's queued or undelivered messages; and it SHALL purge every blob the user's
devices uploaded, reusing `Blobs.DeleteBlob` semantics (blob-store) — one stored copy
per `blob_id`, keyed by the uploading device — where purging an already-expired or
already-deleted blob is a safe no-op. The server SHALL retain only a minimal removed
marker (`users.removed_at`, with the username and no identity material) so the username
can be freed and re-issued and a revoked device stays revoked; it does not retain the
user's identity key, devices, key packages or blobs. Messages already delivered to
other members' devices are **not** erased, because the server holds no history and
E2EE cannot reach other members' copies (config invariant: the server is never the
authority for history). The server enforces the deletion and the retention limit.

#### Scenario: Server-held data is gone
- **WHEN** Bob deletes his account
- **THEN** the server has removed Bob's key packages, push tokens, queued messages and
  uploaded blobs, and `Blobs.DownloadBlob` for one of Bob's former blobs finds nothing

#### Scenario: Minimal removed marker is kept
- **WHEN** Bob deletes his account and the admin later creates an invite for
  `bob@purplechaamba.com`
- **THEN** the server issues the invite against the freed username, and a device with
  no account registers as `bob@purplechaamba.com` with a fresh identity key

#### Scenario: Peers keep what they already received
- **WHEN** Bob deletes his account after sending messages that Alice already received
- **THEN** those messages remain on Alice's device, because the server holds no history
  and cannot erase Alice's copies; the confirmation and `app/store/privacy.md` say so
  plainly

### Requirement: The initiating device wipes itself on success
On an OK `Devices.DeleteAccount` response the client SHALL return the initiating device
to a clean no-account state by invoking the existing device reset (client-runtime:
Device reset, change `replace-user`): it erases the local account, every group and DM,
message history and pending jobs, and the device's identity, auth, MLS and storage keys
from the OS key store, then returns the app to first-run / onboarding. The wipe SHALL
run only after the server confirms the deletion, and the user's other connected devices
SHALL learn of the deletion in-band through the account-removal notice (device-identity:
Account-removal notice, change `replace-user`) so they can reset too. The client
enforces the on-success wipe.

#### Scenario: Local state is erased after deletion
- **WHEN** the server returns OK to Bob's `Devices.DeleteAccount`
- **THEN** Bob's device invokes `reset()`, erasing its account, chats, history and
  key-store keys, and the app returns to the invite / onboarding screen

#### Scenario: A second device learns in-band
- **WHEN** Bob deletes his account from his phone while his tablet is connected
- **THEN** the server delivers the account-removal notice to the tablet before closing
  its stream, and the tablet shows it was removed and can reset

#### Scenario: No wipe without server confirmation
- **WHEN** Bob's `Devices.DeleteAccount` fails (for example the network drops before a
  response)
- **THEN** the client does not wipe local state, and Bob's account and device remain
  usable
