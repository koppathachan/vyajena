## Context

`Text` today is one string. Mentions and replies must stay inside the MLS
ciphertext (the Pi is never the authority for content), must not change the
push text, and must degrade to plain text on the 1.0.0 builds testers have.
The core stores each message's encoded `Content` as it came, so new fields
are kept with the message without a schema change; only "which mentions are
not yet seen" needs a column.

## Goals / Non-Goals

**Goals:** user and `@everyone` mentions with a picker, highlight and badge;
replies with an encrypted quote that work without the original; old clients
keep working.

**Non-Goals:** edit, delete, reactions, threads, notifications per mention,
mention-aware push (the push text never changes), DMs.

## Decisions

### D1. Mentions are spans over the text, in UTF-8 bytes

`Text.mentions` is a list of `Mention { user | everyone, start, length }`.
`start` and `length` are UTF-8 byte offsets into `Text.text` (what Rust and
protobuf use natively). The bridge converts them to UTF-16 code units for
Dart, which `TextSpan` and `TextEditingController` use.

The text still holds what the sender typed (for example "@Ali"), so an old
client shows it unchanged. A span is valid only if it lies inside the text,
on character boundaries, is not empty and does not overlap an earlier span.
The sending core refuses invalid spans (`InvalidMention`); a receiving core
drops invalid ones and keeps the text (enforced by the client). Duplicates of
the same target are allowed (two "@bob" in one message).

Alternative: a markup in the text ("<@alice@pi>"). Rejected: old clients
would show the markup.

### D2. A mention is shown with the reader's name for the target

New clients draw each span as "@" + the reader's own name for the target
(the display name it stores for that user, else the username) or
"@everyone", not the sender's literal text. So a mention always names the
user it really points at, and a later rename shows. Whether a message
mentions *me* is decided from the user ids, never from the text.

### D3. Reply reference: (sender device, `msg_id`), not `server_seq`

`ReplyTo { sender: DeviceRef, msg_id, quote }`.

| | (sender device, `msg_id`) | `server_seq` |
|---|---|---|
| Known when | the message is created | only once the Pi accepts it |
| Reply to an own message still sending | works | impossible |
| Unique | yes (16 random bytes per device; the Pi dedups on it) | yes |
| Lookup on a device | `(sender_device, msg_id)`, the dedup lookup that exists | `server_seq` |
| Who sets it | the sender's core | the Pi |

The device also names who was replied to (`sender.user_id`), so a device
without the original can still say "replying to Ali". Neither field is
authenticated end to end on the original (the Pi sets `sender`, the core
checks it against the MLS leaf; `msg_id` is envelope metadata), so a
dishonest Pi could at worst make "jump to the original" land on the wrong
message. The quote itself is inside the reply's own ciphertext and always
shows what the replier saw.

### D4. The quote: first 100 characters of the original's text

The sending core builds the quote from the stored original:
its text with every run of whitespace (newlines included) folded to one
space, trimmed, cut to `protocol::message_content::MAX_REPLY_QUOTE_CHARS`
(100) Unicode scalar values, the last one "…" when it was cut. A reply's quote
is of the original's text only, never a quote of a quote. An original whose
body this version does not know gets an empty quote. The receiving core cuts
a longer quote to the same limit (defensive; enforced by the client).

### D5. Unseen mentions live in the core

The app may not store anything about messages, so the core does. Migration
`0005_mentions.sql` adds `messages.mention_unseen` (0/1). A received message
from another user that mentions this user or `@everyone` is stored with 1.
`unseen_mentions()` lists their local ids in chat order;
`mark_mention_seen(id)` clears one. Own messages (from any own device) never
count.

The app marks a mention seen once its bubble has been on screen while the
app is in the foreground, or when the user taps the badge (which jumps to the
oldest unseen one). The `@` badge in the app bar shows the count; there is
one chat, so the app bar is "the chat badge". The push text does not change
(D7).

### D6. Reply gestures and jumping

Swipe a bubble to the right (a `Dismissible` that never dismisses) or
long-press it and pick "Reply". The composer shows the quote with a close
button; sending clears it. In the chat, the quote sits above the reply's
text; tapping it scrolls to the original if the core resolved it
(`ReplyQuote.original_id`) and it is among the loaded history, and otherwise
says the original is not on this device.

### D7. Nothing new leaves the device

No Pi, relay or push code changes. Mentions and replies are fields of the
encrypted `Text`, so the envelope, its size class and the wake ping are the
same as for any message: one fixed "New message" (push invariant, change
`push`). Tests show the Pi's push sink records the same text for a
mentioning message.

### D8. Old clients

prost (and every protobuf runtime) skips unknown fields, so a 1.0.0 client
decodes such a `Text` as its `text` alone: mentions show as the typed "@Ali",
a reply shows without its quote. A test decodes a new `Content` with the
1.0.0 `Text` shape.

## Flows

Send a reply with a mention:

```mermaid
sequenceDiagram
  participant UI as Flutter (Alice)
  participant C as Rust core (Alice)
  participant P as Pi
  participant B as Rust core (Bob)
  UI->>C: send_message("@Bob yes", [mention bob 0..4], reply_to = local id 42)
  C->>C: load msg 42: sender device + msg_id, quote = first 100 chars
  C->>C: Content{Text{text, mentions, reply_to}} → MLS encrypt
  C->>P: Envelope APPLICATION (ciphertext only)
  P-->>B: Envelope; wake ping "New message" if offline
  B->>B: decrypt, check spans, mention_unseen = 1 (mentions bob)
  B->>B: resolve reply_to → local id of the original, if stored
  B-->>B: Event::Message (mentions, mentions_me, reply)
```

## Risks / Trade-offs

- [The span offsets are wrong after a UTF-16/UTF-8 conversion bug] → the
  bridge has round-trip tests with emoji and non-Latin text; a bad span is
  dropped, the text still shows.
- [`@everyone` spam] → any member may send it (owner); the push text does
  not change, so it adds no notification noise.
- [Unseen state is per device] → seeing a mention on the phone does not clear
  it on the tablet. Accepted for now (open question 1).

## Migration Plan

Migration `0005_mentions.sql` adds one column with default 0; existing
messages count as seen. No Pi migration, no Pi deploy needed.

## Open Questions

1. Should seeing a mention on one device clear it on the user's other
   devices? Needs an own-devices sync message; not in this change.
2. Should the mention badge also show on the app icon (iOS badge count)?
   The push carries no count today (push invariant); this would be local.
