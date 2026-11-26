## Why

Push today is one generic "New message" visible alert (change `push`), with no
count, no indication of what kind of conversation it is, and no sender. The owner
asked (2026-11-27) for smarter notifications: show a message **count**, distinguish
**DM vs named group vs admin-event**, and **never** put message content in a
notification.

The hard constraint is end-to-end encryption. The server routes ciphertext only,
and a push the server sends travels through APNs or FCM, so **anything in the push
payload leaks to Apple or Google**. The existing content-free wake ping is already
built on this (message-delivery: Content-free wake pings), but it can only ever
carry a fixed string: the server cannot count real messages, name a sender or even
tell a DM from a group without reading content it must never read.

So richness cannot come from the server. It must be computed **on the device**,
after it wakes and decrypts locally, and shown as a **local** notification. The
server push stays a content-free wake signal; the device turns one wake into a
"3 new messages in Work" (or "Message from Ali", if the user opts in) without any
of that ever reaching a push provider. This change makes "no content, sender,
group id or count in the push-provider payload" an explicit, enforced invariant
(the owner's requirement 3 falls out of it) and adds the client-side pipeline that
builds the real notification.

## What Changes

- **New invariant — the wake push payload is content-free and metadata-minimal.**
  The server's push to FCM/APNs SHALL carry no message content, no sender, no
  `group_id` and no message count — only a wake signal plus, as a fallback, the one
  fixed generic string ("New message"). Everything a person sees beyond that
  generic fallback is built on the device. The server enforces the payload; this is
  stated once and both the FCM and APNs paths obey it.
- **Client builds the real notification.** After the device wakes, catches up and
  decrypts locally (message-delivery: Ordered delivery and catch-up), the core
  computes per conversation: an **unread count**, a **type** (DM, named group, the
  lobby, or an admin-event), and — only if the user opted in — a **sender display
  name**. The app shows this as a **local** notification. No decrypted text is ever
  put in it (app-ui: UI goes through the core).
- **Type-aware, content-free wording.** A DM → "2 new messages"; a named group →
  "3 new in Work"; an admin-event → a fixed phrase ("You were removed from Work").
  The group name is known locally (it rides encrypted `GroupSettings`), so it is
  safe in a **local** notification though never in the push payload. No message text
  ever appears.
- **Customizable Basic vs More (owner, 2026-11-27).** A per-device setting
  "Notification detail" lets the user pick **Basic** — the generic "New message"
  only (no count, type or name) — or **More** — the rich per-conversation count and
  type. Default **More** (recommended; the count/type leak nothing off-device), Basic
  the opt-down; the level only changes local rendering, no server/payload change
  (design D10).
- **Sender names are opt-in, off by default (owner decision a).** A local
  per-device setting "Show who messaged" defaults off: notifications stay generic
  ("New message" / "N new in <group>"). Turned on, and only while detail is More, a
  DM/group notification may read "Message from Ali". Default off protects
  who-contacts-whom from a lock-screen glance; see design D6, D10.
- **Admin-events notify for a chosen, content-free set (owner decision c).** Events
  that materially affect the user and are already surfaced as core notices — you
  were removed from a group, a named group was closed, you were added to a new named
  group — raise a notification with fixed wording. Routine membership churn (others
  joining/leaving) does not. See design D4.
- **Platform mechanics make silent-wake reliable or degrade to the generic alert
  (owner decision d).** iOS: a visible alert with `mutable-content` wakes a
  Notification Service Extension that rewrites the notification on-device; if it
  cannot run, the generic "New message" still shows. Android: a high-priority data
  message wakes the app to build the notification; if it cannot run, the next app
  open catches up. The server payload is content-free in both (design D5).
- **Builds on `push-kind-aware` (server already right).** The server already wakes a
  device only for an `APPLICATION` backlog, never for control-only traffic
  (message-delivery: Content-free wake pings, as revised by `push-kind-aware`). This
  change does not re-open that; admin-events that warrant a push are membership
  facts the server knows authoritatively, pushed content-free (design D4).

## Open decisions — resolved (owner, 2026-11-27; detail in design.md)

- **(a) "Message from <username>"** — **accepted:** per-device "Show who messaged",
  default off, opt-in, and only while detail is More (design D6, D10).
- **(b) Count semantics** — **accepted:** per-conversation count ("3 new in Work")
  plus an app-icon badge of total unread (design D3).
- **(c) Which admin-events notify, and their wording** — **accepted:** removed /
  group-closed / added-to-a-new-group only, fixed phrases; own-access events ignore
  per-conv mute but honor mute-all (design D4).
- **(d) Platform mechanics** — **accepted:** iOS alert + `mutable-content` + NSE
  (metadata-only for v1), Android high-priority data message, generic-alert /
  catch-up fallback (design D5, D7).
- **(e) Basic vs More (owner added)** — **accepted:** a per-device "Notification
  detail" setting; default **More** recommended (owner to confirm vs Basic-default);
  no server/payload change (design D10).

## Out of scope

- **Mute** (change `mute-notifications`, already specced): adjacent. This pipeline
  composes with it — a muted conversation is suppressed at the server push and at
  the local build — but muting itself is not implemented here (design D8).
- **Notification reliability** (change `notification-reliability`): the re-ping /
  catch-up scheduling is a separate change; this one assumes the device wakes and
  catches up, and does not change wake scheduling.
- **Decrypted message previews** (showing the text). Explicitly never, by the new
  invariant.

## Capabilities

### New Capabilities

- `local-notifications`: the device builds the user-facing notification locally
  after decrypting — count, type, opt-in sender — and the rule that no decrypted
  content ever leaves the core for it. The client enforces this.

### Modified Capabilities

- `message-delivery`: adds the content-free, metadata-minimal wake-payload
  invariant and the wake shape (alert + `mutable-content` on iOS, data on Android)
  that lets the client enrich, on top of Content-free wake pings.
- `app-ui`: the "Show who messaged" setting, type-aware notification wording, and a
  notification tap opening the right conversation.

## Proto files touched

- **None.** The push payload stays content-free, so no wire field carries any of the
  new information; the wake-shape flags (`mutable-content`, data-vs-alert) are
  push-construction details on the server, not `vyajena.v1` fields. `Envelope.kind`
  and `group_id` already give the client the DM/group/control distinction it needs
  (messaging.proto). If owner decision (d) later calls for an iOS extension that
  connects and counts queued envelopes, that uses the existing `Connect`/catch-up
  RPCs — still no new field. Any proto need discovered at apply time is raised to
  the owner first (config.yaml: proto-first).

## Impact

- **Server:** `server/src/push/` — assert the content-free, metadata-minimal
  payload in one place for both providers; set `mutable-content`/`apns-push-type`
  (iOS) and a data wake (Android) per design D5. The decision of *whether* to push
  (APPLICATION backlog, mute) is unchanged; only the payload shape is pinned. The
  membership-event push of D4 reuses the authoritative membership the server holds.
- **Core:** a notification-model builder that, after catch-up, produces a per
  conversation `{type, unread_count, latest_sender?}` summary for the app, honoring
  the sender-name setting; never emits text. `client/src/engine` + a new small
  module; bridge surface for the summary and the setting.
- **App:** `app/lib/push/` builds local notifications from the core summary;
  Android high-priority data-message handling in `VyajenaMessagingService`; an iOS
  Notification Service Extension target; the "Show who messaged" toggle in settings;
  tap → open conversation. Store/permission impacts coordinated with `app-release`.
- **Docs:** `docs/architecture-decisions.md` "who sees what" gains the explicit
  "push provider sees only a wake" line; this config's context invariant list gains
  the content-free-payload invariant (owner-approved).
- **Ops:** app change (new iOS extension target, Android data messages) plus a
  server payload-shape change; coordinated with `push` (not yet fully deployed).
