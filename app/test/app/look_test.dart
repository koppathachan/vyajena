// app-ui: Arcade look and Arcade motion (change arcade-restyle).
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/app/app.dart';
import 'package:vyajena/app/arcade/arcade_border.dart';
import 'package:vyajena/app/arcade/steps.dart';
import 'package:vyajena/app/arcade/tokens.dart';
import 'package:vyajena/app/theme.dart';

import '../support/fake_core_service.dart';

/// WCAG contrast ratio of two opaque colours.
double contrast(Color a, Color b) {
  final la = a.computeLuminance();
  final lb = b.computeLuminance();
  final hi = la > lb ? la : lb;
  final lo = la > lb ? lb : la;
  return (hi + 0.05) / (lo + 0.05);
}

void main() {
  // spec: app-ui/arcade-look
  test('light and dark come from the one token set', () {
    expect(lightTheme.useMaterial3, isTrue);
    expect(darkTheme.useMaterial3, isTrue);
    expect(darkTheme.colorScheme.brightness, Brightness.dark);
    expect(lightTheme.colorScheme.brightness, Brightness.light);
    expect(darkTheme.scaffoldBackgroundColor, const Color(0xFF0F0F23));
    expect(darkTheme.colorScheme.onSurface, const Color(0xFFFFFFFF));
    expect(darkTheme.cardTheme.color, const Color(0xFF1B1B3A));
    // A duller yellow than #FFFF00 (owner review 2).
    expect(darkTheme.colorScheme.primary, const Color(0xFFE6D84D));
    expect(lightTheme.scaffoldBackgroundColor, const Color(0xFFFFF8E7));
    expect(lightTheme.colorScheme.onSurface, const Color(0xFF0F0F23));
    expect(lightTheme.cardTheme.color, const Color(0xFFFFFFFF));
    expect(lightTheme.colorScheme.primary, const Color(0xFF764BA2));
    expect(ArcadeTokens.dark.border, const Color(0xFFFFFFFF));
    expect(ArcadeTokens.light.border, const Color(0xFF0F0F23));
    expect(ArcadeTokens.light.headingShadowColor, const Color(0xFFFF0000));
    expect(ArcadeTokens.dark.headingShadowColor, const Color(0xFFFF0000));
  });

  // spec: app-ui/arcade-look
  test('square corners, 2px borders and hard shadows', () {
    for (final theme in [lightTheme, darkTheme]) {
      final card = theme.cardTheme.shape! as ArcadeBorder;
      expect(card.width, arcadeBorderWidth);
      expect(card.shadow, cardShadow);
      final button =
          theme.filledButtonTheme.style!.shape!.resolve({})! as ArcadeBorder;
      expect(button.width, arcadeBorderWidth);
      expect(button.shadow, buttonShadow);
      final field = theme.inputDecorationTheme.border! as SketchInputBorder;
      expect(field.borderSide.width, arcadeBorderWidth);
      expect(bubbleShadow, 2);
    }
  });

  // spec: app-ui/arcade-look
  test('dialogs, sheets, menus and snack bars are square and bordered', () {
    for (final theme in [lightTheme, darkTheme]) {
      final dialog = theme.dialogTheme.shape! as ArcadeBorder;
      expect(dialog.width, arcadeBorderWidth);
      expect(dialog.shadow, cardShadow);
      expect(theme.dialogTheme.titleTextStyle?.fontFamily, pixelFont);
      expect(theme.dialogTheme.contentTextStyle?.fontFamily, bodyFont);
      final sheet = theme.bottomSheetTheme.shape! as ArcadeBorder;
      expect(sheet.width, arcadeBorderWidth);
      expect(sheet.shadow, 0);
      final menu = theme.popupMenuTheme.shape! as ArcadeBorder;
      expect(menu.shadow, buttonShadow);
      expect(
        theme.popupMenuTheme.labelTextStyle!.resolve({})?.fontFamily,
        bodyFont,
      );
      expect(theme.snackBarTheme.shape, isA<ArcadeBorder>());
    }
  });

  // spec: app-ui/arcade-look
  test('headings are pixel with a red offset shadow; body is never pixel', () {
    for (final theme in [lightTheme, darkTheme]) {
      final text = theme.textTheme;
      for (final heading in [text.titleLarge!, text.headlineSmall!]) {
        expect(heading.fontFamily, pixelFont);
        // 4px on headlines, 2px on the app-bar title (legibility).
        final offset = heading == text.titleLarge! ? 2.0 : 4.0;
        final red = heading.shadows!.first;
        expect(red.offset, Offset(offset, offset));
        expect(red.blurRadius, 0);
        expect(red.color, const Color(0xFFFF0000));
        // Light mode draws the heading twice, 0.6px apart, so dark-on-light
        // reads as thick as light-on-dark (owner review 2).
        if (theme.brightness == Brightness.light) {
          expect(heading.shadows!.length, 2);
          expect(heading.shadows![1].offset, const Offset(0.6, 0));
          expect(heading.shadows![1].color, heading.color);
        } else {
          expect(heading.shadows!.length, 1);
        }
      }
      for (final body in [text.bodyLarge!, text.bodyMedium!, text.bodySmall!]) {
        expect(body.fontFamily, isNot(pixelFont));
        expect(body.fontFamily, bodyFont);
      }
    }
  });

  // spec: app-ui/arcade-look
  test('text and controls meet WCAG AA contrast on their fills', () {
    for (final t in [ArcadeTokens.light, ArcadeTokens.dark]) {
      final pairs = <String, double>{
        'text on bg': contrast(t.text, t.bg),
        'text on card': contrast(t.text, t.card),
        'heading on bg': contrast(t.heading, t.bg),
        'heading on card': contrast(t.heading, t.card),
        'button text': contrast(t.onButton, t.button),
        'mine bubble': contrast(t.onBubbleMine, t.bubbleMine),
        'theirs bubble': contrast(t.onBubbleTheirs, t.bubbleTheirs),
        'muted on bg': contrast(t.muted, t.bg),
        'muted on card': contrast(t.muted, t.card),
        'error on bg': contrast(t.error, t.bg),
        'error on card': contrast(t.error, t.card),
        'on error': contrast(t.onError, t.error),
        'mention border on bg': contrast(t.mention, t.bg),
        'mention border on card': contrast(t.mention, t.card),
        'focus on bg': contrast(t.focus, t.bg),
        'border on bg': contrast(t.border, t.bg),
        'disabled text': contrast(t.bg, t.disabled),
      };
      pairs.forEach((name, ratio) {
        expect(
          ratio,
          greaterThanOrEqualTo(4.5),
          reason: '$name ${t.brightness}',
        );
      });
    }
  });

  // spec: app-ui/arcade-look
  testWidgets('Dark mode: the app uses the dark tokens', (tester) async {
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
    expect(context.arcade, ArcadeTokens.dark);

    tester.platformDispatcher.platformBrightnessTestValue = Brightness.light;
    await tester.pumpAndSettle();
    final light = tester.element(find.byType(HomeScreen));
    expect(Theme.of(light).colorScheme, lightTheme.colorScheme);
    expect(light.arcade, ArcadeTokens.light);
  });

  // spec: app-ui/arcade-motion
  test('motion is stepped and the text scale is capped', () {
    expect(const Steps(4).transform(0.30), 0.25);
    expect(const Steps(4).transform(0.99), 0.75);
    expect(const Steps(4).transform(1), 1);
    expect(hoverDuration, const Duration(milliseconds: 200));
    expect(entranceDuration, const Duration(milliseconds: 800));
    expect(entranceStagger, const Duration(milliseconds: 120));
    expect(floatDuration, const Duration(seconds: 3));
    expect(const Steps(8), entranceCurve);
    expect(maxTextScale, 1.3);
  });

  // spec: app-ui/arcade-motion
  testWidgets('the text scale is capped at 1.3', (tester) async {
    tester.platformDispatcher.textScaleFactorTestValue = 3;
    addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
    final core = FakeCoreService();
    await tester.pumpWidget(VyajenaApp(core: core));
    await core.open();
    await tester.pump();
    final context = tester.element(find.byType(HomeScreen));
    expect(MediaQuery.textScalerOf(context).scale(10), closeTo(13, 0.01));
  });

  // spec: app-ui/arcade-look
  test('only the pixel font is bundled and one dependency is added', () {
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
      'shared_preferences',
    });
    final flutterSection = pubspec.substring(
      pubspec.lastIndexOf(RegExp(r'^flutter:', multiLine: true)),
    );
    final active = flutterSection
        .split('\n')
        .where((line) => !line.trimLeft().startsWith('#'))
        .join('\n');
    expect(active, contains('family: PressStart2P'));
    expect(
      RegExp(r'family:').allMatches(active).length,
      1,
      reason: 'one bundled font family',
    );
    expect(active, isNot(contains('assets/images')));
    expect(File('assets/fonts/PressStart2P-OFL.txt').existsSync(), isTrue);
  });
}
