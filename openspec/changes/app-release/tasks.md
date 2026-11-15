Start after `qa-fixes` is applied (shared files: `AndroidManifest.xml`,
`pubspec.yaml`, `app.dart`) and after the owner has answered R1–R9 (`proposal.md`).
Groups 1–6 are for a session; group 7 is the owner's alone (accounts, keys,
uploads). No `.proto` changes. Android reuses the old Play app (D16); iOS signing
waits for the Apple account (R2).

## 1. Spikes (throwaway, in `spikes/`, results into design.md)

- [ ] 1.1 S1: on Xcode 27.2, `flutter build ipa --release --no-codesign` (no Apple account needed); if it fails, `flutter build ios --release --no-codesign` + `xcodebuild archive`; record which works. The signed export is re-checked in 7.4 once the account exists (D4)
- [ ] 1.2 S2: release APK with R8 on and a throwaway keystore made in `spikes/`, on the `vyajena_test` emulator: join a temp Pi, send, kill + relaunch, keys still there; with and without the keep rules (D6)
- [ ] 1.3 S3: `llvm-readelf -lW` on the release `libbridge.so` for `arm64-v8a` and `x86_64`: `LOAD` alignment ≥ 16 KB; record sizes of all three; add the linker flag only if needed (D2)
- [ ] 1.4 S4: `nm -u` on the release `libbridge.a` against Apple's required-reason API list; record every category found (D9)
- [ ] 1.5 Delete the spikes and the throwaway keys; record results under D2, D4, D6, D9

## 2. Android

- [ ] 2.0 `build.gradle.kts`: `applicationId = "com.chaamba.vyajena"` per R10, `namespace` unchanged; `run.sh`, README and any `adb`/package references follow (D16)
- [ ] 2.1 `build.gradle.kts`: release signing from `vyajena.keyProperties` (default `~/.vyajena-release/key.properties`); `GradleException` when it is missing; no debug fallback (D3) (app-release: Release signing keys stay with the owner)
- [ ] 2.2 `proguard-rules.pro` with the JNI keep rules, wired into `release` (D6) (app-release: Release build hardening)
- [ ] 2.3 16 KB linker flag in the cargokit Android build if S3 needs it (D2) (app-release: Store architectures)
- [ ] 2.4 `AndroidManifest.xml`: `android:label` from R3; permissions exactly `INTERNET` + `ACCESS_NETWORK_STATE` (app-release: Declared permissions)
- [ ] 2.5 `.gitignore`: key, certificate and profile patterns (D3)

## 3. iOS

- [ ] 3.1 `Info.plist`: `CFBundleDisplayName`/`CFBundleName` from R3, `ITSAppUsesNonExemptEncryption` from R8, `NSLocalNetworkUsageDescription` (D8, D12) (app-release: Encryption export declaration; Declared permissions)
- [ ] 3.2 `Runner/PrivacyInfo.xcprivacy` from D9 + S4 + D10, added to the Runner target (app-release: Privacy manifest)
- [ ] 3.3 `project.pbxproj`: `TARGETED_DEVICE_FAMILY` from R9 (D14); `DEVELOPMENT_TEAM` only after the Apple account exists (R2), else leave for 7.4
- [ ] 3.4 `ios/ExportOptions.plist`; `app/store/build-ipa.sh` only if S1 needs the fallback (D4)

## 4. Shared app

- [ ] 4.1 `app.dart`: `debugShowCheckedModeBanner: false` (D7)
- [ ] 4.2 Icon master, foreground and monochrome in `app/store/icon/`; `app/store/icons.sh` (`sips`); generated Android + iOS icons committed (D11) (app-release: Store listing and icons)
- [ ] 4.3 `app/test/release/` Dart tests on the committed files: manifest permissions and label, `Info.plist` keys and name, `PrivacyInfo.xcprivacy` present with both categories, icon sizes, every log call under `kDebugMode`; `// spec: app-release/<slug>` (Declared permissions; Encryption export declaration; Privacy manifest; Store listing and icons; Release builds log no content)

## 5. Store texts

- [ ] 5.1 `app/store/privacy.md`: the D10 answer sheet (Play Data safety + App Store labels) (app-release: Privacy disclosures match the app)
- [ ] 5.2 `docs/privacy-policy.md` from the sheet: owner, what leaves the phone, where it goes, retention, how to be removed, contact; for the owner to publish at the R5 URL
- [ ] 5.3 `app/store/listing.md`: name, subtitle, descriptions, keywords, category, support URL, age-rating answers (D15)
- [ ] 5.4 `integration_test/screenshots_test.dart` on the fake core; screenshots for iPhone 6.9" and an Android phone into `app/store/screenshots/` (D15) (app-release: Store listing and icons)

## 6. Release check and docs

- [ ] 6.1 `app/store/release-check.sh <aab> <xcarchive>` with every D13 check; `shellcheck` clean; never uploads (app-release: Release check)
- [ ] 6.2 Test it: passes on a bundle + archive signed with S2/S1-style throwaway keys and a matching `signing.md` stand-in; fails on a debug-signed bundle, a bundle missing `armeabi-v7a`, a reused build number, and an extra permission (app-release: Release check; Store architectures; Release signing keys stay with the owner; One version source)
- [ ] 6.3 `docs/release.md`: owner steps in order (retire the old CI deployment, Apple account, keys + backups, Play App Signing, App Store Connect record, export answers, privacy answers, build, check, upload, tag `app-v<name>+<build>`, first build number above the old app's highest versionCode, uninstall the old app first, upload key reset, internal testers, 90-day TestFlight re-upload, re-join after debug builds, re-check store rules at submission)
- [ ] 6.4 README "Release" section (link to `docs/release.md`); CLAUDE.md commands (`release-check.sh`); `docs/architecture-decisions.md`: distribution decisions R1–R9 as the owner answered, and the public-release blockers
- [ ] 6.5 `flutter analyze`, `flutter test`, `cargo test --workspace`, clippy, fmt, `shellcheck app/store/*.sh`, `openspec validate app-release --strict`; debug `flutter run` and `integration_test/run.sh` still work on both simulators

## 7. Owner only (never a session: accounts, keys, external services)

- [ ] 7.0 Retire the old Play deployment (D16): delete `android-internal.yml` and the `ANDROID_KEYSTORE_*`/`ANDROID_KEY_*` secrets in `chaamba/vyajena` and `koppathachan/vyajena`; remove `<play-ci-service-account>` from Play Console users; delete its workload identity provider or the service account in GCP
- [ ] 7.1 Buy the Apple Developer Program membership (R2); note the team id for 3.3
- [ ] 7.2 Move `~/vyajena-upload.jks` under `~/.vyajena-release/`, passwords in the Keychain, backups (R4); its SHA-256 fingerprint into `app/store/signing.md`; after the first vyajena2 upload, request the upload key reset (R11) and update the fingerprint
- [ ] 7.3 Existing Play app `com.chaamba.vyajena`: update listing, Data safety and privacy policy URL from `app/store/`; read its highest versionCode for D5. New App Store Connect record for `com.purplechaamba.vyajena`, filled the same way; publish the privacy policy at the R5 URL
- [ ] 7.4 Build both (iOS: set `DEVELOPMENT_TEAM`, signed `flutter build ipa`), run `app/store/release-check.sh`, upload to Play internal testing now and TestFlight internal testing when the account exists; tag `app-v2.0.0+<build>`
- [ ] 7.5 Testers uninstall the old vyajena first (R12); invite the group (Play internal list, App Store Connect users)
- [ ] 7.6 Real-phone QA on the home Wi-Fi with the store builds: iOS Local Network prompt shows the app's text and joining works; Android keys survive kill + relaunch; device logs have no line from the app and no message text (app-release: Release builds log no content; Declared permissions; Release build hardening)
