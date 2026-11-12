// app-ui: Composer limit.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/chat/composer.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

void main() {
  // spec: app-ui/composer-limit
  testWidgets('Very long paste: nothing is sent, the composer keeps the '
      'text, and the app says it is too long', (tester) async {
    final core = FakeCoreService(storedUser: alice)..maxTextLength = 20;
    await openApp(tester, core);
    final long = 'x' * 21;
    await tester.enterText(find.byKey(const Key('composer')), long);
    await tap(tester, find.byTooltip('Send'));

    expect(core.calls, isNot(contains('sendText')));
    expect(core.sent, isEmpty);
    expect(find.text(tooLongText), findsOneWidget);
    expect(
      tester
          .widget<TextField>(find.byKey(const Key('composer')))
          .controller
          ?.text,
      long,
    );

    // Shortened, it goes.
    await tester.enterText(find.byKey(const Key('composer')), 'x' * 20);
    await tap(tester, find.byTooltip('Send'));
    expect(core.sent, ['x' * 20]);
    expect(find.text(tooLongText), findsNothing);
  });

  // spec: app-ui/composer-limit
  testWidgets('a send the core refuses keeps the text and says why', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: alice)
      ..fails['sendText'] = Exception('the core refused');
    await openApp(tester, core);
    await tester.enterText(find.byKey(const Key('composer')), 'hi');
    await tap(tester, find.byTooltip('Send'));
    expect(find.textContaining('the core refused'), findsOneWidget);
    expect(
      tester
          .widget<TextField>(find.byKey(const Key('composer')))
          .controller
          ?.text,
      'hi',
    );
  });
}
