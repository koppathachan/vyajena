// app-release: what the committed release files must say (design D3–D15).
// These run in CI on every change; app/store/release-check.sh checks the same
// rules again on the built bundle and archive before an upload.
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';

String read(String path) => File(path).readAsStringSync();

const manifest = 'android/app/src/main/AndroidManifest.xml';
const gradle = 'android/app/build.gradle.kts';
const proguard = 'android/app/proguard-rules.pro';
const infoPlist = 'ios/Runner/Info.plist';
const privacyManifest = 'ios/Runner/PrivacyInfo.xcprivacy';
const pbxproj = 'ios/Runner.xcodeproj/project.pbxproj';
const iosIcons = 'ios/Runner/Assets.xcassets/AppIcon.appiconset';
const androidRes = 'android/app/src/main/res';

/// The value after `<key>name</key>` in a plist: the next element's text,
/// or `true`/`false` for a bool. Null when the key is missing.
String? plistValue(String plist, String name) {
  final m = RegExp(
    '<key>${RegExp.escape(name)}</key>\\s*(<true/>|<false/>|<string>([^<]*)</string>)',
  ).firstMatch(plist);
  if (m == null) return null;
  return switch (m.group(1)) {
    '<true/>' => 'true',
    '<false/>' => 'false',
    _ => m.group(2),
  };
}

/// A PNG's width, height and whether it has an alpha channel (IHDR).
({int width, int height, bool alpha}) png(String path) {
  final bytes = File(path).readAsBytesSync();
  final data = ByteData.sublistView(bytes);
  expect(bytes.sublist(12, 16), 'IHDR'.codeUnits, reason: path);
  final colourType = bytes[25];
  return (
    width: data.getUint32(16),
    height: data.getUint32(20),
    alpha: colourType == 4 || colourType == 6,
  );
}

void main() {
  // spec: app-release/declared-permissions
  test('Merged manifest: the app asks for exactly INTERNET and '
      'ACCESS_NETWORK_STATE', () {
    final permissions = RegExp(
      r'<uses-permission\s+android:name="([^"]+)"',
    ).allMatches(read(manifest)).map((m) => m.group(1)).toSet();
    expect(permissions, {
      'android.permission.INTERNET',
      'android.permission.ACCESS_NETWORK_STATE',
      // push design D12: the "New message" notification (asked once).
      'android.permission.POST_NOTIFICATIONS',
      // audio-messages: the microphone for voice messages (asked on first use).
      'android.permission.RECORD_AUDIO',
    });
  });

  // spec: app-release/declared-permissions
  test('First connect on an iPhone: the app dials the relay and asks for '
      'nothing but the microphone, when a voice message is first recorded', () {
    final plist = read(infoPlist);
    final usages = RegExp(
      r'<key>(NS\w+UsageDescription)</key>',
    ).allMatches(plist).map((m) => m.group(1)).toSet();
    // audio-messages: the one thing it asks for is the microphone, on the first
    // voice message.
    expect(usages, {'NSMicrophoneUsageDescription'});
    expect(plist, isNot(contains('UIBackgroundModes')));
    // push design D12: the one capability beyond the default keychain is
    // Push Notifications (aps-environment), in Runner.entitlements only.
    final entitlements = Directory('ios/Runner')
        .listSync()
        .map((e) => e.path)
        .where((p) => p.endsWith('.entitlements'))
        .toList();
    expect(entitlements, [endsWith('Runner.entitlements')]);
    final keys = RegExp(
      r'<key>([^<]+)</key>',
    ).allMatches(read(entitlements.single)).map((m) => m.group(1)).toList();
    expect(keys, ['aps-environment']);
  });

  // spec: app-release/encryption-export-declaration
  test('Upload without a prompt: Info.plist declares '
      'ITSAppUsesNonExemptEncryption = NO (owner R8, 2026-11-16)', () {
    expect(
      plistValue(read(infoPlist), 'ITSAppUsesNonExemptEncryption'),
      'false',
    );
  });

  // spec: app-release/privacy-manifest
  test('Required-reason APIs: the privacy manifest is in the Runner target, '
      'declares no tracking, and gives reasons for file timestamps and disk '
      'space (SQLite, spike S4)', () {
    final manifest = read(privacyManifest);
    expect(plistValue(manifest, 'NSPrivacyTracking'), 'false');
    expect(manifest, contains('NSPrivacyAccessedAPICategoryFileTimestamp'));
    expect(manifest, contains('C617.1'));
    expect(manifest, contains('NSPrivacyAccessedAPICategoryDiskSpace'));
    expect(manifest, contains('E174.1'));
    expect(manifest, isNot(contains('SystemBootTime')));
    expect(
      RegExp(r'PrivacyInfo\.xcprivacy in Resources').allMatches(read(pbxproj)),
      hasLength(2), // the build file and its line in Runner's Resources phase
    );
  });

  // spec: app-release/privacy-disclosures-match-the-app
  test('Answers from one file: the privacy sheet, the policy and the '
      'manifest name the same data', () {
    final sheet = read('store/privacy.md');
    final policy = read('../docs/privacy-policy.md');
    final manifestTypes = RegExp(
      r'<key>NSPrivacyCollectedDataType</key>\s*<string>NSPrivacyCollectedDataType(\w+)</string>',
    ).allMatches(read(privacyManifest)).map((m) => m.group(1)).toSet();
    expect(manifestTypes, {'UserID', 'DeviceID'});
    for (final type in manifestTypes) {
      expect(sheet, contains('NSPrivacyCollectedDataType$type'));
    }
    for (final text in [sheet, policy]) {
      expect(text, contains('end-to-end encrypted'));
      expect(text.toLowerCase(), contains('username'));
      expect(text, contains('IP address'));
      expect(text, contains('7 days'));
      expect(text, contains('New message'), reason: 'push wake-ups (push D12)');
    }
    expect(policy, contains('no tracking'));
    expect(
      read('store/listing.md'),
      contains('https://www.purplechaamba.com/vyajena/privacy/'),
    );
  });

  // spec: app-release/store-listing-and-icons
  test('Same name everywhere: "Vyajena" on Android, iOS and the app', () {
    expect(read(manifest), contains('android:label="Vyajena"'));
    final plist = read(infoPlist);
    expect(plistValue(plist, 'CFBundleDisplayName'), 'Vyajena');
    expect(plistValue(plist, 'CFBundleName'), 'Vyajena');
    expect(read('lib/app/app.dart'), contains("title: 'Vyajena'"));
  });

  // spec: app-release/store-listing-and-icons
  test('Same icon everywhere: every size from the one master, the iOS icons '
      'opaque, the Android adaptive icon with a monochrome layer', () {
    final master = png('store/icon/master.png');
    expect(master, (width: 1024, height: 1024, alpha: false));
    for (final (points, scale) in [
      (20, 2), (20, 3), (29, 1), (29, 2), (29, 3), (40, 2), (40, 3), //
      (60, 2), (60, 3), (1024, 1),
    ]) {
      final icon = png('$iosIcons/Icon-App-${points}x$points@${scale}x.png');
      expect(icon, (
        width: points * scale,
        height: points * scale,
        alpha: false,
      ), reason: '${points}x$points@${scale}x');
    }
    for (final (density, px) in [
      ('mdpi', 48), ('hdpi', 72), ('xhdpi', 96), ('xxhdpi', 144), //
      ('xxxhdpi', 192),
    ]) {
      expect(png('$androidRes/mipmap-$density/ic_launcher.png').width, px);
      for (final layer in ['foreground', 'monochrome']) {
        final image = png('$androidRes/mipmap-$density/ic_launcher_$layer.png');
        expect(image.width, px * 108 ~/ 48, reason: '$density $layer');
        expect(image.alpha, isTrue, reason: '$density $layer');
      }
    }
    final adaptive = read('$androidRes/mipmap-anydpi-v26/ic_launcher.xml');
    expect(adaptive, contains('@mipmap/ic_launcher_foreground'));
    expect(adaptive, contains('@mipmap/ic_launcher_monochrome'));
  });

  // spec: app-release/release-signing-keys-stay-with-the-owner
  test('No upload key: the release build type never falls back to the debug '
      'key and stops without the owner\'s key properties', () {
    final build = read(gradle);
    expect(build, isNot(contains('signingConfigs.getByName("debug")')));
    expect(build, contains('.vyajena-release/key.properties'));
    expect(build, contains('throw GradleException'));
    final ignored = read('../.gitignore');
    for (final pattern in [
      'key.properties',
      '*.jks',
      '*.keystore',
      '*.p12',
      '*.mobileprovision',
    ]) {
      expect(ignored, contains(pattern));
    }
  });

  // spec: app-release/release-build-hardening
  test('Keystore works after shrinking: R8 keeps the JNI class, and no '
      'debug banner or tracking SDK is in the app', () {
    expect(read(gradle), contains('"proguard-rules.pro"'));
    final rules = read(proguard);
    expect(rules, contains('-keep class io.crates.keyring.Keyring { *; }'));
    expect(rules, contains(r'io.crates.keyring.Keyring$Companion'));
    expect(
      read('lib/app/app.dart'),
      contains('debugShowCheckedModeBanner: false'),
    );
    final lock = read('pubspec.lock');
    for (final sdk in [
      'firebase',
      'crashlytics',
      'sentry',
      'google_mobile_ads',
      'analytics',
      'bugsnag',
      'datadog',
      'amplitude',
      'mixpanel',
    ]) {
      expect(lock, isNot(contains(sdk)), reason: sdk);
    }
  });

  // spec: app-release/release-builds-log-no-content
  test('Silent release: every log call in lib/ runs only in debug mode', () {
    final call = RegExp(r'\b(print|debugPrint|log|debugPrintStack)\(');
    final bad = <String>[];
    for (final file in Directory('lib').listSync(recursive: true)) {
      if (file is! File || !file.path.endsWith('.dart')) continue;
      final lines = file.readAsLinesSync();
      for (var i = 0; i < lines.length; i++) {
        if (call.hasMatch(lines[i]) && !lines[i].contains('kDebugMode')) {
          bad.add('${file.path}:${i + 1}');
        }
      }
    }
    expect(bad, isEmpty);
  });

  // spec: app-release/one-version-source
  test('Version reaches both stores: pubspec is the only version, read by '
      'Gradle and Info.plist', () {
    expect(
      RegExp(
        r'^version: \d+\.\d+\.\d+\+\d+$',
        multiLine: true,
      ).hasMatch(read('pubspec.yaml')),
      isTrue,
    );
    final build = read(gradle);
    expect(build, contains('versionCode = flutter.versionCode'));
    expect(build, contains('versionName = flutter.versionName'));
    final plist = read(infoPlist);
    expect(
      plistValue(plist, 'CFBundleShortVersionString'),
      r'$(FLUTTER_BUILD_NAME)',
    );
    expect(plistValue(plist, 'CFBundleVersion'), r'$(FLUTTER_BUILD_NUMBER)');
    expect(
      plistValue(
        read('ios/ExportOptions.plist'),
        'manageAppVersionAndBuildNumber',
      ),
      'false',
    );
  });

  // spec: app-release/store-architectures
  test('iOS archive: iPhone only, and the core is built from source, never '
      'downloaded', () {
    final project = read(pbxproj);
    final families = RegExp(
      r'TARGETED_DEVICE_FAMILY = ([^;]+);',
    ).allMatches(project).map((m) => m.group(1)).toSet();
    expect(families, {'1'});
    // cargokit downloads precompiled binaries only with a cargokit.yaml.
    expect(File('../bridge/cargokit.yaml').existsSync(), isFalse);
    expect(File('rust_builder/cargokit.yaml').existsSync(), isFalse);
  });
}
