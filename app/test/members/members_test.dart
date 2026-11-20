// app-ui: Profile and members.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

void main() {
  // spec: app-ui/profile-and-members
  testWidgets('the members list shows each user with the number of devices', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: alice)
      ..displayNames = {'alice@chat': 'Ali (alice)', 'akhil@chat': 'Akhil (akhil)'}
      ..memberDevices = const [
        MemberDevice(user: akhil, device: '01'),
        MemberDevice(user: alice, device: '02'),
        MemberDevice(user: bob, device: '03'),
        MemberDevice(user: alice, device: '04'),
      ];
    await openApp(tester, core);
    await menu(tester, 'Members');

    Finder row(String name) =>
        find.ancestor(of: find.text(name), matching: find.byType(ListTile));
    expect(
      find.descendant(
        of: row('Akhil (akhil)'),
        matching: find.text('1 device'),
      ),
      findsOneWidget,
    );
    expect(
      find.descendant(
        of: row('Ali (alice) · you'),
        matching: find.text('2 devices'),
      ),
      findsOneWidget,
    );
    expect(
      find.descendant(of: row('bob'), matching: find.text('1 device')),
      findsOneWidget,
    );

    // A member renames: the list follows.
    core.displayNames['bob@chat'] = 'Bobby (bob)';
    core.emit(const CoreEvent.profileChanged(user: bob, displayName: 'Bobby'));
    await settle(tester);
    expect(find.text('Bobby (bob)'), findsOneWidget);
  });
}
