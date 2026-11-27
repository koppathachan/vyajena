## Context

- **The message long-press sheet lives in `message_tile.dart`** (`_sheet`): a
  `showModalBottomSheet` returning a string the widget maps to a callback
  (`onReply`, `onResendImage`, `onDelete`, `onReport`, `onBlock`, …). Entries are
  conditional on which callbacks the owner (`chat_screen.dart`) passed.
- **`chat_screen.dart` owns the callbacks** and the `ChatModel`. It already knows
  the message, the core and the chat. It is also edited by a parallel
  contacts/profile session, so this change keeps its edits to the message-tile
  wiring block small and local.
- **Sending text** goes through `ChatModel.send` → `core.sendText(text)` for the
  lobby (chat 0) or `core.sendMessageIn(chatId, text)` elsewhere. `core.chats()`
  returns `List<ChatSummary>` (chatId, `ChatKindInfo` of lobby/group/dm, closed
  flags) — enough to build a picker and send into any chat.
- **Sharing out** already exists on the `vyajena/platform` method channel
  (`share`), handled in `MainActivity.kt` and `AppDelegate.swift`. The same
  channel carries `dataDir`. Inbound share reuses this channel rather than adding
  a new one.
- **The app root** (`VyajenaApp` in `app.dart`) holds the single `Navigator` and
  the `CoreService`; it is where an inbound share must be caught and turned into a
  picker, since a share can arrive while no chat screen is mounted.

## Goals / Non-Goals

**Goals**
- Copy text (bodies and captions) to the clipboard from the long-press sheet.
- Forward a text message into one other chat via a picker, reusing the send path.
- Make the app an OS share target for text and images, routing inbound content to
  the same picker and the normal send path.
- Keep the E2EE boundary: the iOS extension and the Android intake hold only raw
  shared bytes; all crypto stays in the core, reached through the normal send
  calls.

**Non-goals**
- Forwarding or copying media between chats; sharing to several chats at once;
  inbound video/audio/files.

## Decisions

### D1. Copy is text-only, from body or caption

The sheet shows **Copy** when the message yields text: a `ChatBody_Text`, or a
`ChatBody_Image` whose caption is non-empty (only images carry a caption). `onCopy` is a
`VoidCallback?`; `chat_screen.dart` sets it only for those bodies and, on tap,
does `Clipboard.setData(ClipboardData(text: …))` then a "Copied" snackbar. A
media-only message, a tombstone (`ChatBody_Deleted`) and an unsupported body get
no Copy. The text copied is the same plain string the bubble shows — mentions are
left as their literal `@name` text, as already stored.

Rationale: copying media (clipboard image, save-to-gallery) is a separate feature
with its own permissions; text copy is the owner's ask and needs no core call.

### D2. Forward is text-only this change; the picker is shared

**Forward** shows when the message has forwardable text (same predicate as Copy's
text case, i.e. a text body — not a caption-only forward, since forwarding a photo
without the photo would mislead). `onForward` opens `ForwardPicker`, a
`showModalBottomSheet<ChatSummary>` listing the user's chats from `core.chats()`:

- title per kind: lobby → "lobby"; group → its name or "Unnamed group"; DM → the
  peer's short name (`UserNames`).
- **excluded:** the source chat (can't forward to itself), and closed groups/DMs
  (`ChatKindInfo_Group(closed)`, `ChatKindInfo_Dm(closed)`), which are read-only.
- choosing a chat sends the text there: chat 0 via `core.sendText`, else
  `core.sendMessageIn(chatId, text)` — the same split `ChatModel.send` uses. A
  snackbar names where it went ("Forwarded to lobby").

Mentions and the reply quote are **not** carried: a forward is a new standalone
message; a mention range only resolves against its original chat's members.

Media forward is deferred (see proposal Non-goals). The predicate makes Forward
simply not appear on a media-only message, so the UI never offers something that
isn't wired.

### D3. One picker, reused by inbound share

`ForwardPicker.pick(context, core, names, {excludeChatId})` returns the chosen
`ChatSummary?`. Forward passes the source chat id to exclude it; inbound share
passes none (every open chat is a target). This keeps one tested picker for both
paths.

### D4. Inbound share intake at the app root

A `ShareIntake` (in `share_intake.dart`) wraps the `vyajena/platform` channel:

- `takeInitialShare()` — a Dart→native call the app makes once the core is ready,
  pulling any share that launched the app (cold start) and clearing it natively.
- the channel's Dart handler receives `sharedContent` calls the native side
  invokes when a share arrives while the app runs (Android `onNewIntent`, iOS URL
  open), exposing them as a stream.

Payload shape (a `Map`): `{ "type": "text", "text": "…" }` or
`{ "type": "image", "bytes": <raw image bytes> }` (first item only for
multi-share this change). The **native side reads the shared image and hands its
raw bytes across** — the Dart app never opens a file, keeping the `lib/` "no
`dart:io`, no file API" invariant (app-ui: UI goes through the core; enforced by
`test/app/goes_through_core_test.dart`). The app root listens, and on a payload:
ensure the core is `ready` (a share can arrive before onboarding — if
unregistered, ignore with a snackbar), open `ForwardPicker`, then send — text via
`sendText`/`sendMessageIn`, an image via `core.sendImage(bytes, chatId: …)`. The
image is a *new* attachment from another app, so the normal encode-and-send path
is correct and no re-encode concern applies (unlike forwarding an existing blob).

### D5. Android native

Manifest: a second `<intent-filter>` on `MainActivity` with `ACTION_SEND` and
`ACTION_SEND_MULTIPLE`, `category.DEFAULT`, and `data` mime types `text/plain`
and `image/*`. `MainActivity` captures the share in `onCreate` (from the launch
`intent`) and in `onNewIntent` (warm, `singleTop`): for text, `EXTRA_TEXT`; for an
image, read the `EXTRA_STREAM` content-URI's **bytes** now (content URIs aren't
readable once the sender is gone) and put them in the payload — no file is kept.
The payload is held in a field; the Dart `takeInitialShare` returns and clears it,
and a warm-start share is also pushed with
`channel.invokeMethod("sharedContent", …)`.

### D6. iOS native — owner's signing step, documented

A Share Extension (`ShareExtension/ShareViewController.swift`, `Info.plist` with
`NSExtensionActivationRule` for 1 text or 1 image, `ShareExtension.entitlements`
with the App Group) writes the shared item into the App Group container (text in a
`share.json` manifest, an image's raw bytes in `share.img`) and opens
`vyajena-share://shared` to foreground the host app. The Runner registers the
`vyajena-share` URL scheme and, when it becomes active, drains the App Group
(`SharedInbox.drain` reads and removes the manifest, returning text, or an image's
**bytes** read from `share.img`) and feeds the `vyajena/platform` channel's
`takeInitialShare`, matching the Android payload. Draining on activation (the app
pulls on startup and on each resume) avoids fragile `UIScene` URL-handler
overrides. The app opens no file of its own — only the native `SharedInbox` reads
the App Group, and hands Dart the bytes.

Because a Share Extension is a **new Xcode target** with its **own bundle id,
provisioning profile, entitlements and App Group**, which only the owner's signing
identity can create, this change ships the source and a step-by-step
`ios/ShareExtension/README.md`; the owner adds the target in Xcode, adds the App
Group capability to **both** the Runner and the extension (which updates
`Runner.entitlements`), and sets up the App Group in the Apple Developer portal /
App Store Connect. The App Group is **not** pre-committed to `Runner.entitlements`,
so the release guard that pins the Runner's entitlements to `aps-environment`
stays green until the owner enables it. Android is verified end to end in this
change; iOS is verified once the owner completes signing.

### D7. Tests

Widget tests on the fake core (`test/support`):
- `app-ui/copy-a-message`: long-press a text message → Copy present → tap →
  clipboard holds the text (via the `Clipboard` platform-channel mock) and a
  "Copied" snackbar; Copy absent on a media-only message and a tombstone.
- `app-ui/forward-a-message`: long-press → Forward → the picker lists the other
  chats and excludes the current and closed ones → choosing one calls the core's
  send-into-chat with the text.
- `app-ui/share-into-vyajena`: a simulated inbound text payload opens the picker
  and sends into the chosen chat.

The native intent/extension plumbing is not unit-testable in `flutter test`; it is
verified manually (Android on an emulator now; iOS after the owner's signing) and
the steps are written in the change and the iOS README.

## Risks / Trade-offs

- **Media forward deferred.** Users will expect to forward photos. Mitigated by a
  clear follow-up and by inbound image share (a photo from the gallery app) still
  working. The alternative — re-download + re-encode through `sendImage` — was
  rejected per the owner's "don't re-encode" and to avoid `bridge/`/core edits
  that would collide with the parallel media-fixes session.
- **iOS not fully buildable in-repo** until the owner adds the target. Mitigated by
  shipping complete source + exact steps; nothing else in the app depends on the
  extension, so the app still builds and ships without it.
- **`chat_screen.dart` is shared** with another session. Edits are limited to the
  `MessageTile(...)` argument block and three small handler methods, to minimise
  conflicts.
