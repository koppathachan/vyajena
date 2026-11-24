## Why

End users hit small things worth reporting — a cramped emoji, a notification that
never arrived, a confusing screen — and today there is nowhere in the app to say
so. They would have to find the operator out of band.

The hard constraint is that this is a **published, self-hostable** app
(self-hostable: Published build is operator-agnostic): there is no central
Anthropic/owner inbox and no single operator. Each server has its own operator,
so feedback from a user on operator X's server must reach operator X — not the
app publisher, and not a baked-in address. The owner's email is private and must
never be compiled into the build.

So "send feedback" is really "reach *this* server's operator", and the design
question is which channel, given E2EE (the server reads no message content) and
the no-central-inbox reality. design.md lays out the fork; this proposal encodes
the recommendation and leaves the final call to the owner.

## What Changes

- **A "Send feedback" entry in the app (app-ui).** The Profile tab gains a Send
  feedback option. What it does is driven by the server the install is on: the app
  reads an operator-configured feedback channel from `ServerInfo` and routes the
  user there.
- **An operator-configured feedback channel on `ServerInfo` (recommended; see
  design D1 option B).** A new optional `ServerInfo.feedback` field lets the
  operator declare how their users reach them — an email address, or an external
  URL (an issue tracker, a form) — plus a short label. If the operator configures
  nothing, the app shows that this server has no feedback channel. Nothing is
  hardcoded in the build; the destination comes from the chosen server at runtime,
  exactly like the rest of server identity (self-hostable).
- **The user sees exactly what is sent and to whom (user-feedback capability).**
  Before anything leaves the device the app shows the destination (the operator's
  configured email or URL) and the exact text, and sends only what the user typed
  plus optional, visible, user-toggleable diagnostics (app version, server id,
  platform). No message content, contacts, keys or other PII are attached
  silently. Because the channel is the operator's external email/URL, the feedback
  does not pass through the E2EE message store and the server keeps no durable
  copy (invariant: the server holds no durable history/content).

## Open decisions resolved

- The feedback target is **this server's own operator**, never the app publisher
  or a baked-in address (self-hostable constraint).
- The owner email (and any operator contact) is **operator-configured at runtime**,
  never compiled into the published build.

## Open questions (for the owner)

- **D1 — which channel.** Recommended **option B (operator-configured contact in
  `ServerInfo`)**, because it keeps the server content-free, is genuinely
  self-hostable with nothing hardcoded, and keeps the user in control of what
  leaves the device. Option A (an in-app E2EE message to the admin) and option C
  (a server-stored feedback queue) are laid out with their open questions in
  design.md; C is **rejected** (it would store readable user text on the server,
  breaking the no-durable-content invariant). Owner to confirm B, and whether to
  *also* offer A later. This is the headline call.
- **D2 — the `ServerInfo` field number.** `feedback = 5` is the next free tag, but
  `self-hostable` noted a *deferred* optional `ServerInfo` server-display-name
  field (number TBD). If both are built, reconcile the two tags; `buf breaking`
  gates it. Flagged, not guessed.

## Proto files touched

- `proto/vyajena/v1/devices.proto`: `ServerInfo.feedback = 5`, an optional
  `FeedbackChannel{ FeedbackMethod method = 1; string target = 2; string label =
  3; }` with `enum FeedbackMethod { FEEDBACK_METHOD_UNSPECIFIED = 0;
  FEEDBACK_METHOD_NONE = 1; FEEDBACK_METHOD_EMAIL = 2; FEEDBACK_METHOD_URL = 3; }`.
  Additive, unauthenticated read (`GetServerInfo` is already unauthenticated);
  `buf lint`, `buf breaking` pass; `cargo run -p pbgen`.
- Only option B needs a proto change. Option A would instead need a way for a user
  to learn the admin's user id (e.g. `ServerInfo.admin_user_id`); option C would
  need a `Feedback.Submit` RPC and server storage. Neither is in this change.

## Capabilities

### New Capabilities

- `user-feedback`: feedback reaches this server's own operator through the
  operator-configured channel; the channel is operator-configured and optional;
  the user sees exactly what is sent and to whom, with no silent PII and nothing
  hardcoded.

### Modified Capabilities

- `app-ui`: a Send feedback entry in the Profile tab that uses the server's
  configured feedback channel.

## Impact

- **Proto:** `ServerInfo.feedback = 5` + `FeedbackChannel` / `FeedbackMethod`.
- **Server:** read an operator-set feedback channel from config and return it in
  `GetServerInfo`; no storage, no new RPC. `deploy/server/` gains an optional
  `feedback` config key (email or URL + label); documented as operator-set, never
  committed with a real address. The owner's own server may leave it unset or set
  a public contact — the owner's private email is not placed in the repo.
- **Core/bridge:** expose `ServerInfo.feedback` to the app (the core already
  fetches `ServerInfo`); no new core logic.
- **App:** `app/lib/feedback/` — a Send feedback screen that composes the user's
  text, shows destination + exact payload + toggleable diagnostics, and hands off
  to the OS mail composer (email channel) or opens the URL (url channel); hides or
  disables the entry when the channel is `NONE`/unset.
- **Ops:** operators document their chosen channel. Spec-only here (external-disk
  cargo target serialises builds).
