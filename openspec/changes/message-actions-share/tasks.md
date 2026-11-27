## 0. Before starting

- [x] 0.1 No proto, bridge, core or server change; this is an `app/`-only change
      (Dart + native Android/iOS share plumbing)

## 1. In-app Copy and Forward (Dart)

- [x] 1.1 `message_tile.dart`: add `onCopy` and `onForward` callbacks; show **Copy**
      when the message has text (text body, or image with a caption) and
      **Forward** on a text body; wire them into the long-press sheet
- [x] 1.2 `forward_picker.dart` (new): `ForwardPicker.pick` lists chats from
      `core.chats()` (lobby / group name / DM peer name), excludes a given chat id
      and closed chats, returns the chosen `ChatSummary?`
- [x] 1.3 `chat_screen.dart`: wire `onCopy` (clipboard + "Copied" snackbar) and
      `onForward` (open the picker, send text via `sendText`/`sendMessageIn`, name
      the destination in a snackbar)
- [x] 1.4 Widget tests `app-ui/copy-a-message` and `app-ui/forward-a-message`

## 2. Share target — Dart intake

- [x] 2.1 `platform.dart` + `share_intake.dart` (new): `takeInitialShare()` and a
      `sharedContent` handler over the `vyajena/platform` channel; a payload model
      (`text`, or `image` with raw bytes — the app opens no file)
- [x] 2.2 `app.dart`: start the intake at the app root (also draining on resume);
      on a payload open `ForwardPicker` and send (text, or image via `sendImage`
      with the shared bytes); ignore with a notice when no user is registered
- [x] 2.3 Widget test `app-ui/share-into-vyajena` (simulated inbound payload →
      picker → send)

## 3. Share target — Android native

- [x] 3.1 `AndroidManifest.xml`: `ACTION_SEND` + `ACTION_SEND_MULTIPLE`
      intent-filter for `text/plain` and `image/*` on `MainActivity`
- [x] 3.2 `MainActivity.kt`: capture the share on cold start and `onNewIntent`
      (read an image content-URI's bytes, keeping no file), hold it, serve it to
      `takeInitialShare`, and push warm-start shares via `invokeMethod`
- [ ] 3.3 Verify end to end on an Android emulator (share text and a photo from
      another app into Vyajena); record the steps in the change

## 4. Share target — iOS native (owner finishes signing)

- [x] 4.1 `ios/ShareExtension/` (new): `ShareViewController.swift`, `Info.plist`
      (`NSExtensionActivationRule` for 1 text / 1 image), `ShareExtension.entitlements`
      (App Group) — writes the item to the App Group and opens `vyajena-share://`
- [x] 4.2 `ios/Runner`: `CFBundleURLTypes` for `vyajena-share`, and `SharedInbox`
      draining the App Group on activation into the `vyajena/platform` channel
      (App Group capability added to Runner.entitlements by the owner in Xcode, so
      the release guard stays green until then)
- [x] 4.3 `ios/ShareExtension/README.md`: exact Xcode / Apple Developer steps the
      owner runs (add target, bundle id, App Group, provisioning); note the app
      builds and ships without the extension until then

## 5. Check

- [x] 5.1 `cd app && flutter analyze`, `cd app && flutter test`,
      `openspec validate message-actions-share --strict`
