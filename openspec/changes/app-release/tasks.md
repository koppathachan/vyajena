Start after `qa-fixes` is applied (shared files: `AndroidManifest.xml`,
`pubspec.yaml`, `app.dart`). The owner answered R1–R12 (2026-11-15,
`proposal.md`). Groups 1–6 are for a session; group 7 is the owner's alone
(accounts, keys, uploads). No `.proto` changes. Android: a new Play app (D16);
iOS signing waits for the Apple account (R2).

## 1. Spikes (throwaway, in `spikes/`, results into design.md)

- [x] 1.1 S1: on Xcode 27.2, `flutter build ipa --release --no-codesign` (no Apple account needed); if it fails, `flutter build ios --release --no-codesign` + `xcodebuild archive`; record which works. The signed export is re-checked in 7.4 once the account exists (D4)
- [x] 1.2 S2: release APK with R8 on and a throwaway keystore made in `spikes/`, on a throwaway emulator (never `vyajena_test`): join a temp Pi, send, kill + relaunch, keys still there; with and without the keep rules (D6)
- [x] 1.3 S3: `llvm-readelf -lW` on the release `libbridge.so` for `arm64-v8a` and `x86_64`: `LOAD` alignment ≥ 16 KB; record sizes of all three; add the linker flag only if needed (D2)
- [x] 1.4 S4: `nm -u` on the release `libbridge.a` against Apple's required-reason API list; record every category found (D9)
- [x] 1.5 Delete the spikes and the throwaway keys; record results under D2, D4, D6, D9

## 2. Android

- [x] 2.1 `build.gradle.kts`: release signing from `vyajena.keyProperties` (default `~/.vyajena-release/key.properties`); `GradleException` when it is missing; no debug fallback (D3) (app-release: Release signing keys stay with the owner)
- [x] 2.2 `proguard-rules.pro` with the JNI keep rules, wired into `release` (D6) (app-release: Release build hardening)
- [x] 2.3 16 KB linker flag in the cargokit Android build if S3 needs it (D2) (app-release: Store architectures). Not needed: S3 found the 64-bit `libbridge.so` already 16 KB aligned
- [x] 2.4 `AndroidManifest.xml`: `android:label="Vyajena"` (R3); permissions exactly `INTERNET` + `ACCESS_NETWORK_STATE` (app-release: Declared permissions)
- [x] 2.5 `.gitignore`: key, certificate and profile patterns (D3)

## 3. iOS

- [x] 3.1 `Info.plist`: `CFBundleDisplayName`/`CFBundleName` `Vyajena` (R3), `ITSAppUsesNonExemptEncryption = YES` (R8); no `NSLocalNetworkUsageDescription` (D12: dropped after the relay) (app-release: Encryption export declaration; Declared permissions)
- [x] 3.2 `Runner/PrivacyInfo.xcprivacy` from D9 + S4 + D10, added to the Runner target (app-release: Privacy manifest)
- [x] 3.3 `project.pbxproj`: `TARGETED_DEVICE_FAMILY = 1`, iPhone only (R9, D14); drop `UISupportedInterfaceOrientations~ipad`; `DEVELOPMENT_TEAM` only after the Apple account exists (R2), else leave for 7.4
- [x] 3.4 `ios/ExportOptions.plist`; `app/store/build-ipa.sh` only if S1 needs the fallback (D4). No fallback: `flutter build ipa` works on Xcode 27.2 (S1). Signed export needs one after all: `app/store/build-ipa.sh` (7.4, D4)

## 4. Shared app

- [x] 4.1 `app.dart`: `debugShowCheckedModeBanner: false` (D7)
- [x] 4.2 `app/store/icon/draw.swift` renders "വ്യ" (Malayalam Sangam MN Bold, slate seed colour on light) as master, foreground and monochrome (R3); `app/store/icons.sh` (`sips`); generated Android + iOS icons committed (D11) (app-release: Store listing and icons)
- [x] 4.3 `app/test/release/` Dart tests on the committed files: manifest permissions and label, `Info.plist` keys and name, `PrivacyInfo.xcprivacy` present with both categories, icon sizes, every log call under `kDebugMode`; `// spec: app-release/<slug>` (Declared permissions; Encryption export declaration; Privacy manifest; Store listing and icons; Release builds log no content)

## 5. Store texts

- [x] 5.1 `app/store/privacy.md`: the D10 answer sheet (Play Data safety + App Store labels) (app-release: Privacy disclosures match the app)
- [x] 5.2 `docs/privacy-policy.md` from the sheet: owner, what leaves the phone, where it goes, retention, how to be removed, contact; for the owner to publish at the R5 URL
- [x] 5.3 `app/store/listing.md`: name, subtitle, descriptions, keywords, category, support URL, age-rating answers (D15)
- [x] 5.4 `integration_test/screenshots_test.dart` on the fake core; screenshots for iPhone 6.9" and an Android phone into `app/store/screenshots/` (D15) (app-release: Store listing and icons)

## 6. Release check and docs

- [x] 6.1 `app/store/release-check.sh <aab> <xcarchive>` with every D13 check; `shellcheck` clean; never uploads (app-release: Release check)
- [x] 6.2 Test it: passes on a bundle + archive signed with S2/S1-style throwaway keys and a matching `signing.md` stand-in; fails on a debug-signed bundle, a bundle missing `armeabi-v7a`, a reused build number, and an extra permission (app-release: Release check; Store architectures; Release signing keys stay with the owner; One version source)
- [x] 6.3 `docs/release.md`: owner steps in order (Apple account, keys + backups, Play App Signing, App Store Connect record, export answers, privacy answers, build, check, upload, tag `app-v<name>+<build>`, internal testers, 90-day TestFlight re-upload, re-join after debug builds, re-check store rules at submission, and last: retire the old app and its CI deployment once vyajena2 runs)
- [x] 6.4 README "Release" section (link to `docs/release.md`); CLAUDE.md commands (`release-check.sh`: left to the tracker, which edits CLAUDE.md); `docs/architecture-decisions.md`: distribution decisions R1–R12 as the owner answered, and the public-release blockers
- [x] 6.5 `flutter analyze`, `flutter test`, `cargo test --workspace`, clippy, fmt, `shellcheck app/store/*.sh`, `openspec validate app-release --strict`; debug `flutter run` and `integration_test/run.sh` still work on both simulators

## 7. Owner only (never a session: accounts, keys, external services)

- [ ] 7.1 Buy the Apple Developer Program membership, individual (R2); note the team id for 3.3 and 7.4
- [x] 7.2 New upload keystore for the new app in `~/.vyajena-release/`, passwords in the macOS Keychain, backup in the password manager (R4, R11); its SHA-256 fingerprint into `app/store/signing.md`
- [ ] 7.3 (Play part done 2026-11-15: app created, privacy/support pages live on www.purplechaamba.com; App content forms and App Store Connect still open) New Play app `com.purplechaamba.vyajena` in the existing console (Play App Signing on); later, the App Store Connect record for the same id (France left out, R8); fill both from `app/store/listing.md` and `app/store/privacy.md`; publish the privacy policy at `https://www.purplechaamba.com/vyajena/privacy/` and support at `https://www.purplechaamba.com/vyajena/` (R5)
- [ ] 7.4 (Android done 2026-11-15: 1 (1.0.0) on internal testing, tag app-v1.0.0+1; iOS 2026-11-16: `DEVELOPMENT_TEAM` set, 1.0.0 (2) signed with `app/store/build-ipa.sh`, release check all PASS; App Store Connect record and TestFlight upload wait for the owner) Build both (iOS: set `DEVELOPMENT_TEAM`, signed `flutter build ipa`), run `app/store/release-check.sh`, upload to Play internal testing now and TestFlight internal testing once the Apple account exists; tag `app-v1.0.0+1`
- [ ] 7.5 Invite the group's testers (Play internal list; App Store Connect users later); the old vyajena app can stay installed beside it (R12)
- [ ] 7.6 Real-phone QA with the store builds, on mobile data and on Wi-Fi: iOS joins through the relay with no permission prompt; Android keys survive kill + relaunch; device logs have no line from the app and no message text (app-release: Release builds log no content; Declared permissions; Release build hardening)
- [ ] 7.7 Once vyajena2 is deployed and running on the group's phones (D16): delete the old Play app `com.chaamba.vyajena`; delete `android-internal.yml` and the `ANDROID_KEYSTORE_*`/`ANDROID_KEY_*` secrets in `chaamba/vyajena` and `koppathachan/vyajena-legacy`; remove `<play-ci-service-account>`'s access to the old app only (kept for vyajena2 uploads, D16) and check its workload identity provider's repo condition; testers uninstall the old app
