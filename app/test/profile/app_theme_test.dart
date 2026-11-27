// app-ui: Theme choice (change arcade-restyle, design D15). A theme is the
// look and its colours together: Arcade (default) and Calm.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/app/app.dart';
import 'package:vyajena/app/arcade/app_theme.dart';
import 'package:vyajena/app/arcade/arcade_border.dart';
import 'package:vyajena/app/arcade/motion.dart';
import 'package:vyajena/app/arcade/sketch.dart';
import 'package:vyajena/app/arcade/tokens.dart';
import 'package:vyajena/app/theme.dart';
import 'package:vyajena/app/theme_mode.dart';

import '../shell/app_shell_test.dart' show aliceWithEverything;
import '../support/app.dart';

void main() {
  Future<void> openProfile(WidgetTester tester, ThemeModeStore store) async {
    reduceMotion(tester);
    final core = aliceWithEverything();
    await tester.pumpWidget(VyajenaApp(core: core, themeStore: store));
    await core.open();
    await settle(tester);
    await tap(tester, find.byKey(const Key('tab-profile')));
  }

  MaterialApp app(WidgetTester tester) =>
      tester.widget<MaterialApp>(find.byType(MaterialApp));

  // spec: app-ui/theme-choice
  testWidgets('Arcade by default; picking Calm restyles the app at once and '
      'is kept', (tester) async {
    final store = MemoryThemeModeStore();
    await openProfile(tester, store);
    expect(app(tester).theme, themeFor(AppTheme.arcade, Brightness.light));

    final segment = find.byKey(const Key('app-theme'));
    await tester.scrollUntilVisible(
      segment,
      200,
      scrollable: find
          .descendant(
            of: find.byKey(const Key('profile-list')),
            matching: find.byType(Scrollable),
          )
          .first,
    );
    await tester.tap(find.byKey(const Key('app-theme-calm')));
    await settle(tester);
    expect(
      app(tester).theme!.extension<ArcadeTokens>(),
      ArcadeTokens.calmLight,
    );
    expect(
      app(tester).darkTheme!.extension<ArcadeTokens>(),
      ArcadeTokens.calmDark,
    );
    expect(store.theme, AppTheme.calm);

    await tester.tap(find.byKey(const Key('app-theme-arcade')));
    await settle(tester);
    expect(app(tester).theme!.extension<ArcadeTokens>(), ArcadeTokens.light);
    expect(store.theme, AppTheme.arcade);
  });

  // spec: app-ui/theme-choice
  testWidgets('a kept theme and Dark together give the theme\'s dark palette', (
    tester,
  ) async {
    await openProfile(
      tester,
      MemoryThemeModeStore(ThemeMode.dark, AppTheme.calm),
    );
    expect(app(tester).themeMode, ThemeMode.dark);
    final context = tester.element(find.byKey(const Key('tab-profile')));
    expect(Theme.of(context).extension<ArcadeTokens>(), ArcadeTokens.calmDark);
  });

  // spec: app-ui/theme-choice
  test('Calm is straight, shadowless, still and plain; Arcade is not', () {
    for (final b in Brightness.values) {
      final calm = themeFor(AppTheme.calm, b);
      final card = calm.cardTheme.shape! as ArcadeBorder;
      expect(card.rough, 0);
      expect(card.shadow, 0);
      expect((calm.inputDecorationTheme.border! as SketchInputBorder).rough, 0);
      expect((calm.appBarTheme.shape! as SketchEdgeBorder).rough, 0);
      expect(calm.textTheme.titleLarge!.fontFamily, bodyFont);
      expect(calm.textTheme.titleLarge!.shadows ?? const [], isEmpty);
      final t = AppTheme.calm.tokens(b);
      expect(t.lively, isFalse);

      final arcade = themeFor(AppTheme.arcade, b);
      final box = arcade.cardTheme.shape! as ArcadeBorder;
      expect(box.rough, sketchCorner);
      expect(box.shadow, cardShadow);
      expect(arcade.textTheme.titleLarge!.fontFamily, pixelFont);
      expect(AppTheme.arcade.tokens(b).lively, isTrue);
    }
    // A straight outline is the rectangle itself.
    const rect = Rect.fromLTWH(0, 0, 120, 40);
    expect(sketchOutline(rect, 5, corner: 0).getBounds(), rect);
  });

  // spec: app-ui/theme-choice
  testWidgets('under Calm nothing is turned or bobs', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: themeFor(AppTheme.calm, Brightness.light),
        home: const Column(
          children: [
            Askew(seed: 3, child: SizedBox(key: Key('askew'), width: 10)),
            ArcadeFloat(child: SizedBox(key: Key('float'), width: 10)),
          ],
        ),
      ),
    );
    await tester.pump(const Duration(seconds: 1));
    expect(
      find.ancestor(
        of: find.byKey(const Key('askew')),
        matching: find.byType(Transform),
      ),
      findsNothing,
    );
    final float = tester.widget<Transform>(
      find.ancestor(
        of: find.byKey(const Key('float')),
        matching: find.byType(Transform),
      ),
    );
    expect(float.transform.getTranslation().y, 0);
    expect(tester.hasRunningAnimations, isFalse);
  });
}
