## Why

Vyajena is missing three ordinary messaging niceties the owner asked for
(2026-11-27): **copying** a message's text, **forwarding** a message into another
chat, and the app appearing as a **share target** in other apps' share sheets.

Today a chat message's long-press sheet (`message_tile.dart`) offers only Reply,
Resend, Delete, Report and Block — no Copy, no Forward. The app shares text *out*
only (`platform.dart` `shareText`, the native `ACTION_SEND` / `UIActivityViewController`
in `MainActivity.kt` and `AppDelegate.swift`), for invites and link texts; it has
no *inbound* share, so other apps cannot send a photo or a line of text to
Vyajena.

This is overwhelmingly an app (`app/`) change. Copy and Forward draw on text and
the chat list the core already hands Dart; the share target is native plumbing
that hands *raw* shared content to the running app, where the normal in-app send
path encrypts and sends it. The E2EE boundary is unchanged: no key, ciphertext or
plaintext-to-disk crosses the share extension; it only passes bytes the user
picked to the main app.

## What Changes

- **Copy a message** (owner, 2026-11-27). The long-press sheet gains **Copy** for
  a message with text: a text body, or an image/video with a caption. Copy puts
  the text on the clipboard (`Clipboard.setData`) and shows a "Copied" snackbar.
  Nothing leaves the device. No Copy on a media-only message, a tombstone or an
  unsupported body.
- **Forward a message** (owner, 2026-11-27). The sheet gains **Forward**, which
  opens a picker of the user's *other* chats (the lobby, named groups and open
  DMs, minus the current chat and closed chats) and sends the content there.
  - **Text forward is in scope** and reuses the existing send path
    (`sendText` / `sendMessageIn`): no re-encode, no core change. Mentions and
    the reply quote are dropped — a forward is a fresh message, not a reply, and a
    mention span only makes sense in its own chat's membership.
  - **Media forward (image / video / voice) is out of scope** for this change and
    is called out as a follow-up (see Non-goals): forwarding an existing
    attachment without re-encoding needs a new core `forward` call that re-uploads
    the stored blob, which touches `bridge/src/api/` and the core. Doing it by
    re-downloading and re-sending through `sendImage`/`sendVideo` would re-encode
    and re-upload, which the owner's brief explicitly asked to avoid.
- **Vyajena is a share target** (owner, 2026-11-27). Other apps' share sheets list
  Vyajena for plain text and images; sharing to it opens a chat picker and
  composes the content in the chosen chat.
  - **Android:** an `ACTION_SEND` (+ `ACTION_SEND_MULTIPLE`) intent-filter for
    `text/plain` and `image/*` in `AndroidManifest.xml`; `MainActivity.kt` reads
    the incoming intent (cold start and `singleTop` warm start via `onNewIntent`),
    stores the payload, and hands it to Dart over the existing `vyajena/platform`
    channel. Verified end to end on an emulator.
  - **iOS:** a **Share Extension** target that writes the shared item into a
    shared **App Group** container and opens the host app via a custom URL scheme;
    the Runner reads it and hands it to Dart. The new Xcode target, its bundle id,
    App Group and provisioning are the **owner's** signing step (documented); the
    extension holds no crypto.
  - **Dart:** a share receiver that, on inbound text or an image, routes to the
    chat picker and composes — reusing the Forward picker. An image arriving from
    another app is a *new* attachment, sent through the normal `sendImage` path
    (its designed entry for arbitrary picked bytes), so no re-encode concern
    applies to it.

## Non-goals

- **Forwarding media** (image, video, voice) between chats — a follow-up needing a
  core `forward(messageId, toChatId)` that re-uploads the stored blob without
  re-encoding.
- **Copying media** to the clipboard or saving it to the gallery.
- Forwarding/sharing to **multiple** chats at once (the picker chooses one).
- Inbound share of **video, audio or files** — only `text/plain` and images.
- Any wire, server or proto change.

## Proto files touched

- None. No `.proto`, `protocol/`, server or core change. Copy and text-Forward use
  text and chats the core already exposes; the share target hands raw bytes to the
  existing `sendText` / `sendImage` entry points.

## Capabilities

### Modified Capabilities

- `app-ui`: the message long-press sheet gains Copy and Forward (text); the app
  becomes an OS share target that routes inbound text and images to a chat picker.

## Impact

- **App (`app/`):**
  - `lib/chat/message_tile.dart`: Copy and Forward entries + `onCopy` / `onForward`
    callbacks.
  - `lib/chat/forward_picker.dart` (new): the one-chat picker, reused by Forward
    and by inbound share.
  - `lib/chat/chat_screen.dart`: wire `onCopy` (clipboard + snackbar) and
    `onForward` (pick a chat, send text there).
  - `lib/app/share_intake.dart` (new) + `lib/app/platform.dart`: receive inbound
    shared content and expose it to the app root, which opens the picker.
  - `lib/app/app.dart`: start the share intake at the app root once the core is
    ready.
  - `android/app/src/main/AndroidManifest.xml`, `MainActivity.kt`: inbound share
    intent-filters and handling.
  - `ios/ShareExtension/` (new): the Share Extension; `ios/Runner/AppDelegate.swift`
    + `Info.plist`: the URL scheme and App Group read. Owner finishes signing.
  - `pubspec.yaml`: no new third-party dependency (uses `flutter/services`).
- **No bridge, core, server or proto change.**
- **Depends on:** nothing new. Lands independently; touches only the message-tile
  sheet, a new picker, the app root and native share plumbing.
