// app-ui: Data on the phone. The platform settings that keep the core's
// database private and out of backups, read from the project files (the
// behaviour itself runs only on a phone).
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

String read(String path) => File(path).readAsStringSync();

void main() {
  // spec: app-ui/data-on-the-phone
  test('Android backup: the app sets allowBackup="false" and excludes '
      'everything from cloud backup and device transfer', () {
    final manifest = read('android/app/src/main/AndroidManifest.xml');
    expect(manifest, contains('android:allowBackup="false"'));
    expect(manifest, contains('android:fullBackupContent="false"'));
    expect(
      manifest,
      contains('android:dataExtractionRules="@xml/data_extraction_rules"'),
    );
    final rules = read(
      'android/app/src/main/res/xml/data_extraction_rules.xml',
    );
    for (final section in ['cloud-backup', 'device-transfer']) {
      final body = RegExp(
        '<$section>(.*?)</$section>',
        dotAll: true,
      ).firstMatch(rules)!.group(1)!;
      for (final domain in [
        'root',
        'file',
        'database',
        'sharedpref',
        'external',
      ]) {
        expect(
          body,
          contains('<exclude domain="$domain" path="." />'),
          reason: '$section must exclude $domain',
        );
      }
      expect(body, isNot(contains('<include')));
    }
  });

  // spec: app-ui/data-on-the-phone
  test(
    'Android: the database lives in app-private, never-backed-up storage',
    () {
      final activity = read(
        'android/app/src/main/kotlin/com/purplechaamba/vyajena/MainActivity.kt',
      );
      // The one place the folder is made, shared with the headless wake that
      // runs when a push arrives while the app is closed.
      final wake = read(
        'android/app/src/main/kotlin/com/purplechaamba/vyajena/WakeRunner.kt',
      );
      expect(wake, contains('File(context.noBackupFilesDir, "core")'));
      expect(activity, contains('WakeRunner.coreDataDir(this)'));
      // The Keystore store gets its Context before Flutter starts.
      final onCreate = activity.indexOf('override fun onCreate');
      expect(
        activity.indexOf('Keyring.initializeNdkContext', onCreate),
        lessThan(activity.indexOf('super.onCreate', onCreate)),
      );
    },
  );

  // spec: app-ui/data-on-the-phone
  test('iOS: the database is excluded from iCloud backup and protected with '
      'NSFileProtectionComplete', () {
    final delegate = read('ios/Runner/AppDelegate.swift');
    expect(delegate, contains('.applicationSupportDirectory'));
    expect(delegate, contains('values.isExcludedFromBackup = true'));
    expect(delegate, contains('try dir.setResourceValues(values)'));
    // The folder and every file already in it; new files inherit it.
    expect(
      RegExp(
        r'\.protectionKey: FileProtectionType\.complete',
      ).allMatches(delegate).length,
      greaterThanOrEqualTo(3),
    );
    expect(delegate, isNot(contains('completeUntilFirstUserAuthentication')));
  });

  // spec: app-ui/data-on-the-phone
  test('the core opens its database only in the folder the platform gives', () {
    final service = read('lib/core/rust_core_service.dart');
    expect(service, contains("MethodChannel('vyajena/platform')"));
    expect(service, contains("invokeMethod<String>('dataDir')"));
    expect(service, contains('rust.Core.open(dataDir: dataDir)'));
    for (final native in [
      'ios/Runner/AppDelegate.swift',
      'android/app/src/main/kotlin/com/purplechaamba/vyajena/MainActivity.kt',
    ]) {
      expect(read(native), contains('"vyajena/platform"'));
      expect(read(native), contains('"dataDir"'));
    }
  });

  // spec: app-ui/data-on-the-phone
  test('the app id is com.purplechaamba.vyajena on both platforms', () {
    final gradle = read('android/app/build.gradle.kts');
    expect(gradle, contains('namespace = "com.purplechaamba.vyajena"'));
    expect(gradle, contains('applicationId = "com.purplechaamba.vyajena"'));
    final project = read('ios/Runner.xcodeproj/project.pbxproj');
    final ids = RegExp(
      r'PRODUCT_BUNDLE_IDENTIFIER = ([^;]+);',
    ).allMatches(project).map((m) => m.group(1)).toSet();
    expect(ids, {
      'com.purplechaamba.vyajena',
      'com.purplechaamba.vyajena.RunnerTests',
      // The notification extension (smarter-notifications): the app id + suffix.
      'com.purplechaamba.vyajena.NotificationService',
    });
  });
}
