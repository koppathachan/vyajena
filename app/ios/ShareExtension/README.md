# Vyajena Share Extension (iOS) — owner setup

This folder holds the source for the iOS **Share Extension** that makes Vyajena
appear in other apps' share sheets (change `message-actions-share`). The code is
complete, but a Share Extension is a **new Xcode target** with its **own bundle
id, provisioning profile, entitlements and App Group** — things only your signing
identity can create. Android works out of the box; iOS needs the steps below
once. Until they are done, the app still builds and ships normally — it just
won't show up as a share target on iOS.

The extension holds **no cryptography**. It only copies the raw shared item (text,
or an image file) into the shared **App Group** container and foregrounds the app.
The app reads the item and does all encryption and sending through the Rust core,
so the E2EE boundary is unchanged.

## What's already in the repo

- `ShareViewController.swift` — reads the shared text/image, writes it to the App
  Group (`share.json`, plus `share.img` for an image), and opens
  `vyajena-share://shared` to foreground the app.
- `Info.plist` — the `NSExtension` config (principal class `ShareViewController`,
  activation for 1 image / text / 1 URL).
- `ShareExtension.entitlements` — the App Group `group.com.purplechaamba.vyajena`.
- Runner side (already wired, except the App Group capability you add in step 3):
  `Runner/Info.plist` registers the `vyajena-share` URL scheme, and
  `AppDelegate.swift` (`SharedInbox`) drains the App Group when the app becomes
  active (the Dart side pulls on startup and on each resume) and hands the item —
  text, or an image's **raw bytes** — to Dart over the `vyajena/platform` channel.
  The app opens no file; only `SharedInbox` reads the App Group.

> **Note:** the App Group is **not** pre-committed to `Runner.entitlements` (a
> release test pins the Runner's entitlements to `aps-environment` only). Adding
> the App Group capability in step 3 writes it into `Runner.entitlements` for
> you; that is expected and correct once the feature is enabled.

If you change the App Group id or the URL scheme, change it in **all** of:
`ShareViewController.swift`, `ShareExtension.entitlements`, `Runner.entitlements`,
`Runner/Info.plist` (scheme), and `AppDelegate.swift` (`SharedInbox.appGroup`).

## Steps in Xcode (open `ios/Runner.xcworkspace`)

1. **Add the target.** File ▸ New ▸ Target… ▸ **Share Extension**. Name it
   `ShareExtension`. Product/bundle id: `com.purplechaamba.vyajena.ShareExtension`
   (must be the Runner's id with a suffix). When Xcode asks to activate the
   scheme, say Activate. Set its iOS Deployment Target to match Runner.
2. **Use these files, not the generated ones.** Xcode generates a
   `ShareViewController.swift`, a storyboard and an `Info.plist`. Delete the
   generated `ShareViewController.swift` and storyboard, and add the files from
   this folder to the `ShareExtension` target instead (drag them into the target,
   or set the target's Info.plist build setting to this `Info.plist`). The
   principal-class setup means no storyboard is needed.
3. **App Group.** In the Apple Developer portal (Certificates, Identifiers &
   Profiles ▸ Identifiers ▸ App Groups), create
   `group.com.purplechaamba.vyajena` if it doesn't exist. Then, for **both** the
   `Runner` and `ShareExtension` targets: Signing & Capabilities ▸ + Capability ▸
   **App Groups** ▸ check `group.com.purplechaamba.vyajena`. (The entitlement
   files already list it; this links them to the provisioning profiles.)
4. **Signing.** Under Signing & Capabilities for `ShareExtension`, pick your Team
   and let Xcode manage signing, or select the matching provisioning profile.
   Both the app id and the extension app id, plus the App Group, must be on the
   profiles.
5. **Build settings.** Confirm the extension's `Info.plist` is this folder's file
   and its `Code Signing Entitlements` is `ShareExtension/ShareExtension.entitlements`.
6. **Run.** Build the Runner scheme to a device/simulator. Share text or a photo
   from Photos/Safari and pick **Vyajena**: it foregrounds and opens the chat
   picker.

## App Store Connect

- The extension ships **inside** the app; it needs no separate App Store listing.
- Make sure the App Group and the extension's bundle id exist in your account
  before you archive, or the upload will fail code-signing.
- Export compliance is unchanged — the extension adds no cryptography.

## How it flows

1. User taps Share ▸ Vyajena in another app.
2. `ShareViewController` writes the item to the App Group and opens
   `vyajena-share://shared`.
3. iOS foregrounds Vyajena. On becoming active, Dart calls `takeInitialShare`;
   `SharedInbox.drain()` reads and removes `share.json` and returns the payload.
4. Dart's `ShareIntake` opens the chat picker and sends the content through the
   core.
