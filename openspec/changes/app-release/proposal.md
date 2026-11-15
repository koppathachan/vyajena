## Why

The app runs only as debug builds on simulators and the emulator. The owner asked
(2026-11-15) to get it ready to publish on Android (Google Play) and iOS (App
Store / TestFlight). Today a release build would be signed with the debug key,
carry Flutter's default icon and the name "vyajena", has no store declarations
(export compliance, privacy manifest, Data safety, privacy labels), and has never
been archived for a real device. On a real iPhone it would also hit the Local
Network prompt with no usage text, because the Pi is reached at `192.168.1.8`.

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
  (from `connectivity_plus`, added by `qa-fixes`); iOS `NSLocalNetworkUsageDescription`
  for the home-LAN phase, nothing else.
- **Store declarations:** `ITSAppUsesNonExemptEncryption` set in `Info.plist`
  (the app does its own end-to-end encryption with standard algorithms:
  OpenMLS, TLS 1.3 via rustls, XChaCha20-Poly1305); `PrivacyInfo.xcprivacy` for the
  required-reason APIs the Rust core uses; Play Data safety and App Store privacy
  label answers written down once, from what actually leaves the phone (ciphertext,
  usernames, public keys and device certificates, IP address, all only to the
  owner's Pi).
- **Privacy policy and listing text:** a privacy policy page, store listing text
  and screenshots (from the fake core, never real messages), committed under
  `app/store/`.
- **Release checklist:** `app/store/release-check.sh` checks a built AAB and IPA
  (architectures, alignment, signer fingerprint, permissions, plist keys, version)
  and `docs/release.md` lists the manual steps in order.

No `.proto` changes. No invariant changes. Resolves none of the architecture
doc's numbered decisions; adds the release decisions below.

### What blocks a real public release today

- **No relay yet:** invites name `192.168.1.8:8443`, so the app works only on the
  owner's home Wi-Fi. App Review and Play review cannot reach the Pi, so they
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

### Owner answers so far (2026-11-15)

- **Android:** the old vyajena's Play app is already on internal testing; reuse
  it. Remove the old public repo's deployment (the `android-internal` GitHub
  Actions workflow, its signing secrets and its keyless Google access).
- **iOS:** no Apple Developer account yet; it has to be bought first.

Reusing the Play app has consequences that need a yes (R10–R12): Play never
lets an app change its package name, and the old app is `com.chaamba.vyajena`.
Updates must be signed with the upload key the old app used (`~/vyajena-upload.jks`,
which was also in GitHub secrets). Every versionCode must be higher than the
highest the old CI uploaded (it used the run number + 100).

### Open decisions (the owner's)

| # | Decision | Recommendation |
|---|---|---|
| R1 | Public store listing vs private distribution for one group | **Android answered:** Play internal testing on the existing app. iOS: TestFlight internal testing once the account exists (≤100 testers, no review); public only after the relay, account deletion and report/block exist |
| R2 | Apple Developer Program and Play Console accounts: who, which type, cost | **Play answered:** the existing console. Apple: the owner buys an individual account (US$99/year); an organization account (needs a D-U-N-S number) only if the developer name must not be the owner's legal name. iOS signing, archive and upload wait for it |
| R3 | App display name (and icon idea) | "Vyajena" on both stores and home screens, a plain monogram icon in the app's muted seed colour |
| R4 | Who holds signing keys, where backups live | The owner alone: Android upload keystore in `~/.vyajena-release/` (outside the repo, never in CI) with its password in the macOS Keychain; one encrypted backup of keystore + password on the external drive and in the owner's password manager; Play App Signing holds the app key. iOS: Xcode-managed distribution certificate in the login keychain, exported `.p12` backed up the same way |
| R5 | Privacy policy URL | `https://purplechaamba.com/vyajena/privacy` (owner's domain), the page's source committed in the repo |
| R6 | Minimum OS versions | Keep today's: Android 7.0 (API 24, Flutter's default), iOS 15.0; raise only if a group member's phone is newer anyway. Target API 36 (Play's 2026 rule) |
| R7 | Release before the relay exists? | Yes, privately (R1), to the group's own phones on the home Wi-Fi, so real-phone QA starts now; no public listing before the relay |
| R8 | Export compliance answer | `ITSAppUsesNonExemptEncryption = YES`, then answer App Store Connect as standard algorithms under the mass-market exemption (no CCATS); exclude France from distribution or file its declaration. Owner confirms; not legal advice |
| R9 | iPhone only or iPhone + iPad | iPhone only (`TARGETED_DEVICE_FAMILY = 1`): iPads still run it, and no iPad screenshots or iPad review are needed |
| R10 | Android application id, to reuse the Play app | `applicationId = "com.chaamba.vyajena"` on Android only; the Kotlin `namespace` and iOS bundle id stay `com.purplechaamba.vyajena`. (Alternative: a new Play app for `com.purplechaamba.vyajena` in the same console, no fee, testers re-invited) |
| R11 | Upload key after removing the old CI | Keep `~/vyajena-upload.jks` for the first upload, then ask Play for an upload key reset to a new key that has never been in CI, held as in R4. The old key sat in GitHub secrets |
| R12 | Phones with the old vyajena installed | Uninstall the old app before installing vyajena2. A store update over it would leave the old app's data and keys behind, and nothing reads them |

## Capabilities

### New Capabilities

- `app-release`: how a store build is made and what it must declare: release
  architectures, signing key custody, versioning, hardening, permissions, export
  compliance, privacy manifest and disclosures, listing assets, release check.

### Modified Capabilities

None. (`app-ui` keeps its requirements; `qa-fixes` is changing three of them
now, and this change does not touch them.)

## Impact

- `app/android/`: `app/build.gradle.kts` (`applicationId` per R10, release
  signing from an outside file, R8 rules), `app/proguard-rules.pro` (new), `AndroidManifest.xml` (label,
  `ACCESS_NETWORK_STATE` check), launcher icon resources.
- `app/ios/`: `Info.plist` (display name, `ITSAppUsesNonExemptEncryption`,
  `NSLocalNetworkUsageDescription`), `Runner/PrivacyInfo.xcprivacy` (new),
  `project.pbxproj` (team, device family), app icon set, `ExportOptions.plist`.
- `app/lib/app/app.dart`: `debugShowCheckedModeBanner: false`.
- `app/pubspec.yaml`: version only. No new dependencies.
- `app/store/` (new): icon master + `icons.sh`, listing text, privacy answers,
  screenshots, `release-check.sh`.
- `docs/release.md` (new), `docs/privacy-policy.md` (new), README "Release"
  section, `docs/architecture-decisions.md` (distribution + release decisions).
- `.gitignore`: `key.properties`, `*.jks`, `*.keystore`, `*.p12`, `*.mobileprovision`.
- Outside the repo, done by the owner only: the Apple account, keys,
  certificates, App Store Connect / Play Console records, uploads, and removing
  the old repo's Play deployment (workflow, `ANDROID_KEYSTORE_*` secrets, the
  `play-internal-ci` service account's Play access and its workload identity
  binding).
- Runs after `qa-fixes` lands in `app/` (shared files: `AndroidManifest.xml`,
  `pubspec.yaml`, `app.dart`).
