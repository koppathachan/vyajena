// app-ui: Light Material 3 look (flutter-app design D8).
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/app/app.dart';
import 'package:vyajena/app/theme.dart';

import '../support/fake_core_service.dart';

void main() {
  // spec: app-ui/light-material-3-look
  test('light and dark schemes come from one muted seed, neutral variant', () {
    expect(lightTheme.useMaterial3, isTrue);
    expect(darkTheme.useMaterial3, isTrue);
    expect(
      lightTheme.colorScheme,
      ColorScheme.fromSeed(
        seedColor: seedColor,
        dynamicSchemeVariant: DynamicSchemeVariant.neutral,
      ),
    );
    expect(
      darkTheme.colorScheme,
      ColorScheme.fromSeed(
        seedColor: seedColor,
        brightness: Brightness.dark,
        dynamicSchemeVariant: DynamicSchemeVariant.neutral,
      ),
    );
    expect(lightTheme.colorScheme.brightness, Brightness.light);
    expect(darkTheme.colorScheme.brightness, Brightness.dark);
    // Muted: the seed is a low-saturation slate.
    expect(HSLColor.fromColor(seedColor).saturation, lessThan(0.25));
    expect(themeMode, ThemeMode.system);
  });

  // spec: app-ui/light-material-3-look
  testWidgets('Dark mode: the app uses the dark scheme from the same seed', (
    tester,
  ) async {
    tester.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
    addTearDown(tester.platformDispatcher.clearPlatformBrightnessTestValue);
    final core = FakeCoreService();
    await tester.pumpWidget(VyajenaApp(core: core));
    await core.open();
    await tester.pump();

    final app = tester.widget<MaterialApp>(find.byType(MaterialApp));
    expect(app.themeMode, ThemeMode.system);
    final context = tester.element(find.byType(HomeScreen));
    expect(Theme.of(context).colorScheme, darkTheme.colorScheme);

    tester.platformDispatcher.platformBrightnessTestValue = Brightness.light;
    await tester.pumpAndSettle();
    expect(
      Theme.of(tester.element(find.byType(HomeScreen))).colorScheme,
      lightTheme.colorScheme,
    );
  });

  // spec: app-ui/light-material-3-look
  test('no custom fonts, images or theme packages', () {
    final pubspec = File('pubspec.yaml').readAsStringSync();
    final dependencies = RegExp(
      r'^dependencies:\n((?:[ #].*\n|\n)*)',
      multiLine: true,
    ).firstMatch(pubspec)!.group(1)!;
    final names = RegExp(
      r'^  ([a-z_]+):',
      multiLine: true,
    ).allMatches(dependencies).map((m) => m.group(1)).toSet();
    // The bridge and what its generated code needs, the network state
    // (qa-fixes D6), the gallery picker (media-blobs) and the URL launcher (links); nothing for the
    // look.
    // (qa-fixes D6), the gallery picker (media-blobs) and the microphone and
    // speaker (audio-messages); nothing for the look.
    expect(names, {
      'flutter',
      'bridge',
      'flutter_rust_bridge',
      'freezed_annotation',
      'connectivity_plus',
      'image_picker',
      'url_launcher',
      'record',
      'just_audio',
      'video_player',
    });
    final flutterSection = pubspec.substring(
      pubspec.lastIndexOf(RegExp(r'^flutter:', multiLine: true)),
    );
    final active = flutterSection
        .split('\n')
        .where((line) => !line.trimLeft().startsWith('#'))
        .join('\n');
    expect(active, isNot(contains('fonts:')));
    expect(active, isNot(contains('assets:')));
    expect(lightTheme.textTheme.bodyMedium?.fontFamily, isNot('Roboto Mono'));
  });
}
