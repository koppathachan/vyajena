## Why

The group chat is one flat stream. Members cannot point a message at
someone ("@bob, can you…"), call everyone, or answer one message among many.
The owner put mentions and replies first in the next block of work
(2026-11-17).

## What Changes

- **Mentions (proto, core, app):** a `Text` carries a list of mentions, each a
  user id or "everyone", with where it sits in the text. Typing `@` in the
  composer opens a member picker (display name shown, user id stored).
  Mentions of this device's user (and `@everyone`) are highlighted, and the
  chat's app bar shows an `@` badge with the number of mentions not yet seen.
- **@everyone:** any member may send it (owner, 2026-11-17).
- **Replies (proto, core, app):** a `Text` may carry `reply_to`: a reference to
  the original (its sender device and `msg_id`) plus a short quote of it, at
  most 100 characters, inside the encryption (owner, 2026-11-17). A device
  that never had the original still shows who and what was replied to.
  Swipe a message right, or long-press it, to reply; the quote shows above
  the reply, and tapping it jumps to the original when this device has it.
- **Nothing new for the server:** everything rides inside the encrypted
  `Content`. The server, the relay and Apple/Google learn nothing new, and the
  push text stays exactly "New message" (push invariant); a mention never
  changes it.
- **Old clients:** new fields are unknown fields to them; they show the text,
  `@` names included, as plain text and drop the quote.
- **Out of scope (owner, 2026-11-17):** editing or deleting messages.
- Details and decisions: `design.md`.

## Open decisions resolved

- Who may send `@everyone`: any member (owner, 2026-11-17).
- Reply quote: about 100 characters, inside the encryption (owner,
  2026-11-17); exact rule in design D4.
- Reply reference: (sender device, `msg_id`), not `server_seq` (design D3).
- Edit and delete: not in this change (owner, 2026-11-17).

## Proto files touched

- `proto/vyajena/v1/content.proto`: `Text.mentions`, `Text.reply_to`, new
  `Mention` and `ReplyTo` messages (imports `common.proto`). Additive only:
  `buf breaking` passes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `message-content`: `Text` may carry mentions and a reply reference; new
  requirements for mentions, replies, and what stays inside the encryption.
- `client-runtime`: the core resolves replies and keeps which mentions of
  this user are not yet seen.
- `app-ui`: mention picker, highlighted mentions and the `@` badge, replies.

## Impact

- `proto/`, regenerated `pb.rs` files (no server code changes).
- `protocol/src/message_content.rs` (new): the quote limit.
- `client/`: content encode/decode, a store migration (`mention_unseen`),
  `send_message`, `unseen_mentions`, `mark_mention_seen`.
- `bridge/`: new types and calls; regenerated frb bindings.
- `app/`: composer, message tile, chat screen, chat model, fake core.
