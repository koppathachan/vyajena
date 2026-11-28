// app-ui: Arcade motion (change arcade-restyle).
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/app/app.dart';
import 'package:vyajena/app/arcade/motion.dart';
import 'package:vyajena/app/theme_mode.dart';

import '../shell/app_shell_test.dart' show aliceWithEverything;
import '../support/app.dart';

/// How far down the first chat-list row is drawn (0 once it has entered).
double firstRowOffset(WidgetTester tester) {
  final shift = tester.widget<FractionalTranslation>(
    find
        .descendant(
          of: find.byType(ArcadeEntrance).first,
          matching: find.byType(FractionalTranslation),
        )
        .first,
  );
  return shift.translation.dy;
}

void main() {
  setUp(resetArcadeEntrances);

  // spec: app-ui/arcade-motion
  testWidgets('the chat list slides in once, in steps', (tester) async {
    final core = aliceWithEverything();
    await tester.pumpWidget(
      VyajenaApp(core: core, themeStore: MemoryThemeModeStore()),
    );
    await core.open();
    for (var i = 0; i < 3; i++) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    // Mid-entrance: below its place; later, in place.
    final early = firstRowOffset(tester);
    expect(early, greaterThan(0));
    // The offset moves in steps (8 of them), never in between.
    final stepped = (0.4 - early) / 0.4 * 8;
    expect((stepped - stepped.round()).abs(), lessThan(1e-6));
    await tester.pump(const Duration(seconds: 2));
    expect(firstRowOffset(tester), 0);
    // Ending ArcadeFloat's loop lets the app settle for the next test.
    await tester.pumpWidget(const SizedBox());
  });

  // spec: app-ui/arcade-motion
  testWidgets('under reduce-motion nothing slides in', (tester) async {
    reduceMotion(tester);
    final core = aliceWithEverything();
    await tester.pumpWidget(
      VyajenaApp(core: core, themeStore: MemoryThemeModeStore()),
    );
    await core.open();
    for (var i = 0; i < 3; i++) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(find.byType(ArcadeEntrance), findsWidgets);
    expect(
      find.descendant(
        of: find.byType(ArcadeEntrance),
        matching: find.byType(AnimatedBuilder),
      ),
      findsNothing,
    );
    expect(find.text('#lobby'), findsOneWidget);
    await tester.pumpAndSettle();
  });

  // spec: app-ui/arcade-motion
  testWidgets('the message list has no entrance motion', (tester) async {
    final core = aliceWithEverything();
    core.receive(bob, 'hello', chatId: 1, emitEvent: false);
    await tester.pumpWidget(
      VyajenaApp(core: core, themeStore: MemoryThemeModeStore()),
    );
    await core.open();
    for (var i = 0; i < 8; i++) {
      await tester.pump(const Duration(milliseconds: 300));
    }
    await tester.tap(find.byKey(const ValueKey('chat-1')));
    await tester.pump(const Duration(seconds: 1));
    await tester.pump(const Duration(seconds: 1));
    // The chat screen is on top: its messages are not wrapped.
    expect(find.text('hello'), findsOneWidget);
    expect(
      find.ancestor(
        of: find.text('hello'),
        matching: find.byType(ArcadeEntrance),
      ),
      findsNothing,
    );
    await tester.pumpWidget(const SizedBox());
  });
}
