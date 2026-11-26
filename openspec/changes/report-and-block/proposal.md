## Why

Both app stores require a user-facing moderation layer before a public release.
Apple's **Guideline 1.2 (Safety — User-Generated Content)** and **Google Play's
UGC policy** demand, for any app where users see each other's content, all of:
a way to filter objectionable content, a way to flag/report it, a way to block
an abusive user, and a published commitment that the developer acts on reports
(ejecting offenders) within 24 hours. vyajena2 is invite-only and E2EE, but it
still carries user-generated messages (groups, the lobby, DMs), so both stores
treat it as UGC. The owner confirmed (2026-11-26) this is a **public-release
blocker** on both stores, tracked alongside in-app account deletion (a separate
change).

The hard part — removing a user — is already built. The admin (owner) can eject
or replace any user through the `group-membership` commit authority, reused by
`replace-user` and `remove-closed-dm`. This change does **not** redefine removal.
It adds only the thin user-facing layer on top: let a user hide an abusive user
locally (block), let a user send the admin a report, and write down the policy
by which the admin acts on reports. Enforcement is the removal that already
exists.

The owner's chosen design (2026-11-26, simplest that clears both stores):

- **Block = a local hide list.** Purely per-device, in the core's local store,
  no server marker and no crypto change — the `mute-notifications` marker pattern
  but LOCAL-only. The client hides a blocked user's messages and DMs and raises
  no in-app notification for them.
- **Report = a structured DM to the admin.** Reuse the existing DM path (`dms`:
  Opening a DM) and the reply-quote formatting (`message-content`: Replies, the
  merged `video-messages` work). No new wire message.
- **Removal = the existing admin capability.** Report → the admin reviews →
  the admin ejects the offender with `Admin.RemoveUser` (`group-membership`:
  Removing a user). Plus a written policy (review and act within 24h; a contact
  email).

## What Changes

- **A user can block another user, per device.** The core keeps a local
  *blocked-users set* in SQLite, keyed by `UserId`. `block`/`unblock` add and
  remove an id. The set is device-local: it is **not** synced to the server or to
  the user's other devices (MVP; a synced server-side marker is deferred to
  Phase-2, design D4).
- **Blocked users are hidden and silenced.** The core filters a blocked user's
  group and lobby messages out of the displayed history and suppresses their
  in-app notifications; a DM with a blocked user is hidden from the chat list.
  Unblocking restores them. The messages still arrive and stay stored (nothing is
  dropped); they are only hidden from display.
- **A user can report a message or a user.** A "Report" action on a message and on
  a user composes a structured report and sends it to the **admin** as a DM,
  reusing `dms` and the reply-quote format. Two cases:
  - **group / lobby content** — the admin is already a member of that group and
    holds the message, so the report carries a *reference*: the reported user id,
    the group id, the offending `msg_id`, and a reason.
  - **DM content** — the admin is not in that DM, so the report *quotes* the
    offending message content (the reporter's device has the plaintext) folded and
    capped by the existing reply-quote rules, plus the reported user id and reason.
- **The admin acts on reports (existing capability).** The report arrives as a DM
  in the admin's app. The admin reviews it and, if warranted, removes the user with
  `Admin.RemoveUser` — the removal that already exists. This change adds **no** new
  enforcement RPC.
- **A written moderation policy** (review and act within 24h; a contact email)
  ships as part of the change (design.md), so both stores' "act on reports"
  requirement is satisfied and the text is ready to publish in the app and on the
  store listings.
- **MVP scope.** Local block + report-to-admin DM + the existing removal. No
  server, proto or MLS change. Anything needing one (a synced block marker; a
  typed `Report` wire body; suppressing the content-free wake push for a blocked
  sender) is deferred to Phase-2 in design.md, not pulled into the MVP.

## Open decisions resolved

- Block mechanism: local per-device hide list, no server marker (owner,
  2026-11-26). Server-synced block deferred (design D4).
- Report channel: structured DM to the admin, reusing the DM path and reply-quote
  (owner, 2026-11-26). Typed `Report` wire body deferred (design D3).
- Enforcement: reuse the existing `group-membership` removal; no new RPC (owner,
  2026-11-26).
- Policy: review and remove within 24h, with a contact email, published in-app and
  on both store listings (owner, 2026-11-26).

## Proto files touched

- **None.** The MVP is app + client-core only. Report delivery reuses the existing
  DM path and `Content.Text` with its `ReplyTo` quote (`proto/vyajena/v1/content.proto`,
  unchanged); block is local to the core's SQLite. A typed `Report` body in
  `content.proto` and a server-side block marker in `prefs.proto` are Phase-2 only
  (design D3, D4) and are **not** part of this change. `buf lint` / `buf breaking`
  have nothing to check.

## Capabilities

### New Capabilities

- `moderation`: blocking a user (local hide list), filtering a blocked user's
  content and notifications, reporting a message or a user to the admin over a DM,
  and the policy that the admin acts on reports with the existing removal.

### Modified Capabilities

- `app-ui`: a Block/Unblock affordance on a user, a Report affordance on a message
  and on a user, a blocked marker, and the moderation/policy entry in the Profile
  tab.

## Impact

- **Proto:** none (MVP).
- **Server:** none (MVP). The content-free wake push is unchanged; a blocked
  sender's message may still wake the recipient's device, but the app shows nothing
  and raises no in-app notification (design D4 covers the Phase-2 server marker that
  would also suppress the push).
- **Core:** a local `blocked_users(user_id)` table (migration in the core's SQLite,
  device-local); `Client::block_user(user_id)` / `unblock_user(user_id)` and a
  `blocked_users()` view; the display/event path filters blocked senders and hides
  their DMs; `Client::report(target)` composes the report and sends it to the admin
  through the existing DM send path. Tests `moderation/block-a-user`,
  `moderation/a-blocked-user-is-hidden`, `moderation/report-a-message`,
  `moderation/report-a-dm-message`, `moderation/report-reaches-the-admin`.
- **Bridge/app:** `block_user` / `unblock_user` / `report`; a Block/Unblock action
  and a Report action on messages and user rows; a blocked indicator; a short
  "Safety & moderation" entry in the Profile tab linking the policy.
- **Docs:** the moderation policy text (design.md), to be published in-app and on
  the App Store and Play listings.
- **Ops:** none. No migration on the server, no data change; the core migration is
  local and additive.
