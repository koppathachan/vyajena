// app-ui: Profile and members. The standalone members list was retired by
// `shell-nav-consistency`; the Contacts tab now lists every member with their
// device count, and a tap opens that member's profile.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

final contactsTab = find.byKey(const Key('tab-contacts'));

void main() {
  // spec: app-ui/profile-and-members
  testWidgets('the Contacts tab shows each user with the number of devices', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: alice)
      ..displayNames = {
        'alice@chat': 'Ali (alice)',
        'akhil@chat': 'Akhil (akhil)',
      }
      ..memberDevices = const [
        MemberDevice(user: akhil, device: '01'),
        MemberDevice(user: alice, device: '02'),
        MemberDevice(user: bob, device: '03'),
        MemberDevice(user: alice, device: '04'),
      ];
    await openHome(tester, core);
    await tap(tester, contactsTab);

    Finder row(String username) =>
        find.byKey(ValueKey('contact-$username'));
    expect(
      find.descendant(of: row('akhil'), matching: find.text('1 device')),
      findsOneWidget,
    );
    // Alice has two devices in the group.
    expect(
      find.descendant(of: row('alice'), matching: find.text('2 devices')),
      findsOneWidget,
    );
    expect(find.byKey(const ValueKey('you-alice')), findsOneWidget);
    expect(
      find.descendant(of: row('bob'), matching: find.text('1 device')),
      findsOneWidget,
    );

    // A member renames: the list follows.
    core.displayNames['bob@chat'] = 'Bobby (bob)';
    core.profiles['bob@chat'] = const ProfileInfo(displayName: 'Bobby');
    core.emit(const CoreEvent.profileChanged(user: bob, displayName: 'Bobby'));
    await settle(tester);
    expect(find.text('Bobby'), findsOneWidget);
  });
}
