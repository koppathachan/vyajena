## Why

The app runs only as debug builds on simulators and the emulator. The owner asked
(2026-11-15) to get it ready to publish on Android (Google Play) and iOS (App
Store / TestFlight). Today a release build would be signed with the debug key,
carry Flutter's default icon and the name "vyajena", has no store declarations
(export compliance, privacy manifest, Data safety, privacy labels), and has never
been archived for a real device. On a real iPhone it would also hit the Local
Network prompt with no usage text, because the server is reached at `<server-address>`.

## What Changes

- **Release builds of the core:** cargokit builds `bridge` in release mode from
  source for every store architecture: Android `arm64-v8a`, `armeabi-v7a`,
  `x86_64` in one App Bundle, with 16 KB-aligned `libbridge.so`; iOS `arm64`
  device only. No precompiled binaries are downloaded.
- **Android signing:** the release build signs with the owner's upload key, read
  from a properties file outside the repo; Play App Signing holds the app signing
  key. The release build fails instead of falling back to the debug key.
- **iOS signing and archive:** automatic signing with the owner's team, an App
  Store Connect export via `flutter build ipa` (spike first: Xcode 27.2 breaks
  `flutter build ios --simulator`; device builds are untested).
- **Versioning:** `version:` in `pubspec.yaml` is the only source of
  versionName/versionCode and CFBundleShortVersionString/CFBundleVersion; the
  build number only ever goes up, across both stores.
- **App name and icons:** one display name on both platforms (today Android says
  "vyajena", iOS "Vyajena"); real launcher icons (Android adaptive + themed
  monochrome, iOS 1024 px without alpha) generated from one committed master by a
  script, no new dependency.
- **Release hardening:** R8 keep rules so the JNI class `io.crates.keyring.Keyring`
  and the bridge survive shrinking; no debug banner; no analytics or crash SDK;
  release builds log nothing at all, keeping the invariant "Nothing logs content".
- **Permissions declared:** Android exactly `INTERNET` and `ACCESS_NETWORK_STATE`
  (from `connectivity_plus`, added by `qa-fixes`); iOS none (the home-LAN
  `NSLocalNetworkUsageDescription` was dropped once the relay went live, design D12).
- **Store declarations:** `ITSAppUsesNonExemptEncryption` set in `Info.plist`
  (the app does its own end-to-end encryption with standard algorithms:
  OpenMLS, TLS 1.3 via rustls, XChaCha20-Poly1305); `PrivacyInfo.xcprivacy` for the
  required-reason APIs the Rust core uses; Play Data safety and App Store privacy
  label answers written down once, from what actually leaves the phone (ciphertext,
  usernames, public keys and device certificates, IP address, all only to the
  owner's server).
- **Privacy policy and listing text:** a privacy policy page, store listing text
  and screenshots (from the fake core, never real messages), committed under
  `app/store/`.
- **Release checklist:** `app/store/release-check.sh` checks a built AAB and IPA
  (architectures, alignment, signer fingerprint, permissions, plist keys, version)
  and `docs/release.md` lists the manual steps in order.

No `.proto` changes. No invariant changes. Resolves none of the architecture
doc's numbered decisions; records the owner's release decisions below.

### What blocks a real public release today

- **No relay yet:** invites name `<server-address>:<server-port>`, so the app works only on the
  owner's home Wi-Fi. App Review and Play review cannot reach the server, so they
  cannot sign in, and both require reviewer access for login-gated apps.
  Next planned changes: `qa-verification`, then relay + real push.
- **No account deletion in the app:** both stores require apps that create
  accounts (`Devices.Register` from an invite) to let the user delete their
  account from inside the app (Apple 5.1.1(v); Play's account deletion policy,
  with a web link). Needs its own change with a `.proto` RPC.
- **No report/block for user content:** Apple 1.2 asks chat apps for a way to
  report and block; only the admin can remove a user today.
- **Play production access:** a personal developer account made after
  2023-11-13 needs a closed test with 12+ testers for 14 days first.
- **No real push yet:** a public user expects notifications; they come with the
  relay change.

None of these block private distribution (TestFlight internal testing, Play
internal testing), which needs no review and works on the home LAN.

### Owner decisions (all answered 2026-11-15)

| # | Decision | Owner's answer |
|---|---|---|
| R1 | Public listing vs private distribution | Private: Play internal testing now, TestFlight internal testing once the Apple account exists. Public only after the relay, account deletion and report/block |
| R2 | Developer accounts | Play: the owner's existing console. Apple: an individual account (US$99/year), not bought yet; every iOS signing step waits for it |
| R3 | Display name and icon | "Vyajena" on both stores and home screens. Icon: the Malayalam syllable "വ്യ" (vya; owner changed it from "വ" on 2026-11-15) in the muted slate seed colour on a light background |
| R4 | Signing keys and backups | The owner alone. New Android upload keystore in `~/.vyajena-release/` (outside the repo, never in CI), passwords in the macOS Keychain, one backup in the owner's password manager. Play App Signing holds the app key. iOS distribution certificate in the login keychain, its `.p12` in the password manager |
| R5 | Privacy policy and support URLs | `https://www.purplechaamba.com/vyajena/privacy/` and `https://www.purplechaamba.com/vyajena/`; page source committed, the owner publishes it |
| R6 | Minimum OS versions | Android 7.0 (API 24), iOS 15.0; target API 36 |
| R7 | Release before the relay? | Yes, privately (R1), to the group's phones on the home Wi-Fi |
| R8 | Export compliance | `ITSAppUsesNonExemptEncryption = NO` (was YES; changed 2026-11-16 on App Store Connect's answer, design D8); App Store Connect answers: standard algorithms, mass-market exemption, no CCATS; France left out of availability (not legal advice) |
| R9 | iPhone or iPad | iPhone only (`TARGETED_DEVICE_FAMILY = 1`) |
| R10 | Play app | A **new** Play app for `com.purplechaamba.vyajena` in the existing console (no fee; testers invited afresh). The old app `com.chaamba.vyajena` stays until vyajena2 is deployed and running, then the owner deletes it and its old repo deployment (the `android-internal` workflow, its signing secrets, the `play-internal-ci` service account's access) |
| R11 | Upload key | A new key made for the new app (R4); the old `~/vyajena-upload.jks` stays with the old app and goes with it |
| R12 | Phones with the old vyajena | Different package, so both apps install side by side; testers uninstall the old one when the owner retires it |

## Capabilities

### New Capabilities

- `app-release`: how a store build is made and what it must declare: release
  architectures, signing key custody, versioning, hardening, permissions, export
  compliance, privacy manifest and disclosures, listing assets, release check.

### Modified Capabilities

None. (`app-ui` keeps its requirements; `qa-fixes` is changing three of them
now, and this change does not touch them.)

## Impact

- `app/android/`: `app/build.gradle.kts` (release signing from an outside
  file, R8 rules), `app/proguard-rules.pro` (new), `AndroidManifest.xml` (label,
  `ACCESS_NETWORK_STATE` check), launcher icon resources.
- `app/ios/`: `Info.plist` (display name, `ITSAppUsesNonExemptEncryption`), `Runner/PrivacyInfo.xcprivacy` (new),
  `project.pbxproj` (team, device family), app icon set, `ExportOptions.plist`.
- `app/lib/app/app.dart`: `debugShowCheckedModeBanner: false`.
- `app/pubspec.yaml`: version only. No new dependencies.
- `app/store/` (new): icon master + `icons.sh`, listing text, privacy answers,
  screenshots, `release-check.sh`.
- `docs/release.md` (new), `docs/privacy-policy.md` (new), README "Release"
  section, `docs/architecture-decisions.md` (distribution + release decisions).
- `.gitignore`: `key.properties`, `*.jks`, `*.keystore`, `*.p12`, `*.mobileprovision`.
- Outside the repo, done by the owner only: the Apple account, keys,
  certificates, App Store Connect / Play Console records, uploads, and, once
  vyajena2 runs, deleting the old Play app and its repo deployment (workflow,
  `ANDROID_KEYSTORE_*` secrets, the `play-internal-ci` service account's Play
  access and its workload identity binding).
- Runs after `qa-fixes` lands in `app/` (shared files: `AndroidManifest.xml`,
  `pubspec.yaml`, `app.dart`).
