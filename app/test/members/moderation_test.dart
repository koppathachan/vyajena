// app-ui: Block and unblock a user; Report a message or a user (change
// `report-and-block`). The per-user Report and Block/Unblock actions on the
// member profile (reached by tapping a member on the Contacts tab) and the
// blocked marker. The standalone members list was retired by
// `shell-nav-consistency`.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

final contactsTab = find.byKey(const Key('tab-contacts'));

FakeCoreService group() => FakeCoreService(storedUser: alice)
  ..dms = true
  ..memberDevices = const [
    MemberDevice(user: akhil, device: '01'),
    MemberDevice(user: alice, device: '02'),
    MemberDevice(user: bob, device: '03'),
  ]
  ..displayNames = {
    'alice@chat': 'Ali (alice)',
    'bob@chat': 'Bobby (bob)',
    'akhil@chat': 'akhil',
  };

/// Opens Bob's profile from the Contacts tab.
Future<void> openBobsProfile(WidgetTester tester, FakeCoreService core) async {
  await openHome(tester, core);
  await tap(tester, contactsTab);
  await tap(tester, find.byKey(const Key('contact-bob')));
}

void main() {
  // spec: app-ui/block-and-unblock-a-user
  testWidgets('block then unblock a user from their profile', (tester) async {
    final core = group();
    await openBobsProfile(tester, core);

    await tap(tester, find.byKey(const ValueKey('block-bob')));
    expect(find.byKey(const Key('block-confirm')), findsOneWidget);
    await tap(tester, find.byKey(const Key('block-confirm-ok')));

    expect(core.calls, contains('blockUser'));
    // A blocked marker beside the device count.
    expect(find.textContaining('blocked'), findsOneWidget);

    await tap(tester, find.byKey(const ValueKey('unblock-bob')));
    expect(core.calls, contains('unblockUser'));
    expect(find.textContaining('blocked'), findsNothing);
  });

  // spec: app-ui/report-a-message-or-a-user
  testWidgets('report a user from their profile', (tester) async {
    final core = group();
    await openBobsProfile(tester, core);

    await tap(tester, find.byKey(const ValueKey('report-bob')));
    await tester.enterText(
      find.byKey(const Key('report-reason-field')),
      'abuse',
    );
    await tap(tester, find.byKey(const Key('report-reason-send')));

    expect(core.calls, contains('reportUser'));
    expect(core.lastReport?.reported, bob);
    expect(find.text('Report sent'), findsOneWidget);
  });
}
