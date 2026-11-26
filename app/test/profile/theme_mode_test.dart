// app-ui: Arcade look: the Auto/Light/Dark choice on the Profile tab.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/app/app.dart';
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

  ThemeMode appMode(WidgetTester tester) =>
      tester.widget<MaterialApp>(find.byType(MaterialApp)).themeMode!;

  // spec: app-ui/appearance-choice
  testWidgets('Auto follows the phone by default; Dark and Light override it '
      'and are kept', (tester) async {
    final store = MemoryThemeModeStore();
    await openProfile(tester, store);
    expect(appMode(tester), ThemeMode.system);

    final segment = find.byKey(const Key('theme-mode'));
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
    await tester.tap(find.text('Dark'));
    await settle(tester);
    expect(appMode(tester), ThemeMode.dark);
    expect(store.mode, ThemeMode.dark);

    await tester.tap(find.text('Light'));
    await settle(tester);
    expect(appMode(tester), ThemeMode.light);
    expect(store.mode, ThemeMode.light);

    await tester.tap(find.text('Auto'));
    await settle(tester);
    expect(appMode(tester), ThemeMode.system);
  });

  // spec: app-ui/appearance-choice
  testWidgets('a kept choice is applied when the app starts', (tester) async {
    await openProfile(tester, MemoryThemeModeStore(ThemeMode.dark));
    expect(appMode(tester), ThemeMode.dark);
  });
}
