## ADDED Requirements

### Requirement: Store architectures
A release build SHALL contain the Rust core (`bridge`), built in release mode from
this repository's source, for every architecture the stores install: the Android
App Bundle SHALL carry `libbridge.so` for `arm64-v8a`, `armeabi-v7a` and `x86_64`,
each with 16 KB-aligned `LOAD` segments, and the iOS archive SHALL carry `arm64`
device code only, with no simulator slice. The build SHALL NOT download a
precompiled core. The release check enforces this on the built files.

#### Scenario: Android bundle
- **WHEN** the admin runs `flutter build appbundle --release` and the release check
- **THEN** the bundle has `lib/arm64-v8a/libbridge.so`,
  `lib/armeabi-v7a/libbridge.so` and `lib/x86_64/libbridge.so`, every `LOAD`
  segment of the 64-bit libraries is aligned to 16 KB, and the check passes

#### Scenario: iOS archive
- **WHEN** the admin archives the app for App Store Connect
- **THEN** the app binary in the archive is `arm64` only and links the release
  `libbridge.a`

#### Scenario: Missing architecture
- **WHEN** a bundle lacks `lib/armeabi-v7a/libbridge.so`
- **THEN** the release check fails and names the missing architecture

### Requirement: Release signing keys stay with the owner
Release builds SHALL be signed only with keys the owner holds outside the
repository: the Android upload key from a keystore whose path and passwords come
from a properties file outside the repo, and the iOS distribution certificate
from the owner's keychain. No keystore, certificate, provisioning profile,
password or App Store Connect / Play API key SHALL be committed or given to CI.
The Android release build SHALL fail when the upload key is missing, never fall
back to the debug key. Gradle and the release check enforce this; `.gitignore`
keeps key files out of git.

#### Scenario: No upload key
- **WHEN** the admin runs `flutter build appbundle --release` on a machine without
  the owner's key properties file
- **THEN** the build stops with an error naming the missing file, and no bundle
  signed with the debug key is produced

#### Scenario: Signed by the upload key
- **WHEN** the release check reads the bundle's signer
- **THEN** its SHA-256 certificate fingerprint equals the upload key fingerprint
  recorded in `app/store/signing.md`, and the debug key's fingerprint fails the
  check

#### Scenario: Key file in the tree
- **WHEN** a file named `key.properties` or ending in `.jks`, `.keystore`, `.p12` or
  `.mobileprovision` is in the working tree
- **THEN** `git status` does not list it, and the release check fails if git
  tracks one

### Requirement: One version source
The app's version SHALL come only from `version:` in `app/pubspec.yaml`: the name
SHALL become Android `versionName` and iOS `CFBundleShortVersionString`, and the
build number SHALL become Android `versionCode` and iOS `CFBundleVersion`. The
build number SHALL be higher than the one in the last `app-v*` release tag. The
release check enforces it.

#### Scenario: Version reaches both stores
- **WHEN** `pubspec.yaml` says `version: 1.0.0+3` and the admin builds both apps
- **THEN** the bundle has versionName `1.0.0` and versionCode `3`, and the archive
  has CFBundleShortVersionString `1.0.0` and CFBundleVersion `3`

#### Scenario: Build number reused
- **WHEN** the last release tag is `app-v1.0.0+3` and `pubspec.yaml` still says
  `1.0.0+3`
- **THEN** the release check fails and says to raise the build number

### Requirement: Release build hardening
A release build SHALL run with the shrinker on and keep everything the Rust core
needs at run time: the JNI class `io.crates.keyring.Keyring` and its
`initializeNdkContext` method, and the Flutter and bridge entry points. It SHALL
show no debug banner and SHALL contain no analytics, advertising or crash-reporting
SDK. Gradle's keep rules and the release check enforce this.

#### Scenario: Keystore works after shrinking
- **WHEN** Bob installs the release bundle on an Android phone and joins from the
  admin's invite
- **THEN** `MainActivity` hands the context to the core, the core stores Bob's keys
  in the Android Keystore, and `Devices.Register` succeeds

#### Scenario: No tracking SDK
- **WHEN** the release check lists the bundle's and archive's libraries and classes
- **THEN** none is a known analytics, advertising or crash-reporting SDK

### Requirement: Release builds log no content
A release build SHALL write no log line from Dart or from the Rust core: Dart logs
only when `kDebugMode`, and the core installs no `tracing` subscriber in the app.
Debug builds keep the event-kind line only. The app enforces this; the release QA
run checks the device log.

#### Scenario: Silent release
- **WHEN** Alice, on a release build, receives "hi" from Bob through a
  `ServerFrame` and replies "hey"
- **THEN** the device log (logcat, or the iOS console) has no line written by the
  app's Dart code or the core, and contains neither "hi" nor "hey"

### Requirement: Declared permissions
The Android release manifest SHALL request exactly `INTERNET` and
`ACCESS_NETWORK_STATE`. The iOS app SHALL declare `NSLocalNetworkUsageDescription`
while invites name a home-network address, and no other usage description,
background mode or capability beyond the keychain. The release check enforces
this on the merged manifest and the built `Info.plist`.

#### Scenario: Merged manifest
- **WHEN** the release check reads the bundle's merged `AndroidManifest.xml`
- **THEN** it lists `android.permission.INTERNET` and
  `android.permission.ACCESS_NETWORK_STATE` and no other permission

#### Scenario: First connect on an iPhone
- **WHEN** Bob, on a real iPhone on the home Wi-Fi, joins from an invite naming
  `192.168.1.8:8443`
- **THEN** iOS asks for local network access with the app's own reason text, and
  after Bob allows it `Devices.Register` succeeds

### Requirement: Encryption export declaration
The iOS app SHALL declare `ITSAppUsesNonExemptEncryption` in `Info.plist` with the
value the owner chose (design D8), so no upload stops at the export question, and
`docs/release.md` SHALL record the matching App Store Connect answers: the app
implements end-to-end encryption with standard published algorithms (MLS RFC
9420, TLS 1.3, Ed25519, X25519, AES-GCM, ChaCha20-Poly1305) and no proprietary
cryptography. The release check enforces the key.

#### Scenario: Upload without a prompt
- **WHEN** the admin uploads the archive to App Store Connect
- **THEN** the build reaches TestFlight processing without asking the export
  compliance question again

### Requirement: Privacy manifest
The iOS app SHALL ship a `PrivacyInfo.xcprivacy` that declares no tracking, no
tracking domains, the data types that match the App Store privacy answers, and
a reason for every required-reason API the app's own code and the Rust core call
(at least file timestamps and system boot time). The release check enforces that
the file is in the archive.

#### Scenario: Required-reason APIs
- **WHEN** the core's SQLite store reads file timestamps and Rust's `Instant` reads
  the boot clock in the release archive
- **THEN** the privacy manifest lists `NSPrivacyAccessedAPICategoryFileTimestamp`
  and `NSPrivacyAccessedAPICategorySystemBootTime` with their reasons, and App
  Store Connect reports no missing reason

### Requirement: Privacy disclosures match the app
The disclosures SHALL say the same thing in the privacy policy, the Play Data
safety answers and the App Store privacy answers, kept in one committed file, and SHALL match
what leaves the phone: message content, group settings and profiles only as
end-to-end encrypted ciphertext; the username, identity and device public keys,
device certificates and KeyPackages, the IP address, and (once push exists) a push
token, all only to the owner's Pi; nothing to the developer otherwise, nothing
sold, shared or used for tracking or ads. The policy SHALL name its owner and say
how to have one's account and data removed. A change that sends new data off the
phone SHALL update these answers in the same change. Reviewers of each change
enforce this.

#### Scenario: Answers from one file
- **WHEN** the admin fills in Play's Data safety form and App Store privacy
  "nutrition label"
- **THEN** every answer comes from `app/store/privacy.md`, and the policy at the
  owner's URL says the same

#### Scenario: Push token later
- **WHEN** a later change makes the core send a real push token with
  `Devices.UpdatePushToken`
- **THEN** that change updates `app/store/privacy.md`, the policy and the privacy
  manifest

### Requirement: Store listing and icons
The app SHALL carry one display name on both platforms, the launcher icons
generated from one committed master image (Android adaptive with a monochrome
layer, iOS 1024 px without alpha), and the store listing text and screenshots
committed under `app/store/`. Screenshots SHALL come from the app running on the
fake core, never from a real group's messages. The release check enforces the
name and the icon sizes.

#### Scenario: Same name everywhere
- **WHEN** Bob installs the app on Android and Alice on an iPhone
- **THEN** both home screens show the same display name and the same icon

#### Scenario: Screenshots show no real chat
- **WHEN** the admin regenerates the store screenshots
- **THEN** they show the fake core's sample users and messages only

### Requirement: Release check
The repository SHALL have one release check, `app/store/release-check.sh`, that
the admin runs on the built bundle and archive before every upload, and that
fails on any rule of this capability it can check from the files: architectures
and alignment, signer, key files tracked by git, version, permissions, plist
keys, privacy manifest, name and icons. It SHALL be `shellcheck` clean and
SHALL never upload anything. `docs/release.md` lists the manual steps that
the check cannot do.

#### Scenario: All good
- **WHEN** the admin runs `app/store/release-check.sh` on a bundle and an archive
  that follow every rule
- **THEN** it prints one line per check, all passing, and exits 0

#### Scenario: Debug-signed bundle
- **WHEN** the admin runs it on a bundle signed with the debug key
- **THEN** it fails on the signer check, exits non-zero, and uploads nothing
