// app-ui: Profile and members.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

void main() {
  // spec: app-ui/profile-and-members
  testWidgets('Alice renames herself: the core sends a Profile', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: alice);
    await openHome(tester, core);
    await openProfileTab(tester);
    await tester.enterText(find.byKey(const Key('displayName')), 'Ali');
    await tap(tester, find.text('Save'));
    expect(core.calls, contains('setProfile'));
    expect(core.args['setProfile'], (displayName: 'Ali', about: null));
  });

  // spec: app-ui/profile-and-members
  testWidgets('Alice renames herself: everyone sees "Ali (alice)"', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: bob);
    await openApp(tester, core);
    core.receive(alice, 'hi');
    await settle(tester);
    expect(find.text('alice'), findsOneWidget);

    // Alice's Profile reaches Bob's core, which emits ProfileChanged.
    core.displayNames['alice@chat'] = 'Ali (alice)';
    core.emit(const CoreEvent.profileChanged(user: alice, displayName: 'Ali'));
    await settle(tester);
    expect(find.text('Ali (alice)'), findsOneWidget);
  });

  // spec: app-ui/profile-and-members
  testWidgets('the username is shown read-only with the saved profile', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: alice)
      ..profiles = {
        'alice@chat': const ProfileInfo(displayName: 'Ali', about: 'tea'),
      };
    await openHome(tester, core);
    await openProfileTab(tester);
    final username = tester.widget<TextField>(
      find.descendant(
        of: find.byKey(const Key('username')),
        matching: find.byType(TextField),
      ),
    );
    expect(username.controller?.text, 'alice@chat');
    expect(username.readOnly, isTrue);
    expect(username.enabled, isFalse);
    expect(
      tester
          .widget<TextField>(find.byKey(const Key('displayName')))
          .controller
          ?.text,
      'Ali',
    );
    expect(
      tester.widget<TextField>(find.byKey(const Key('about'))).controller?.text,
      'tea',
    );
  });
}
