// app-ui: Block and unblock a user; Report a message or a user (change
// `report-and-block`). The per-user Report and Block/Unblock actions on the
// members list (the user's profile surface) and the blocked marker.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

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

void main() {
  // spec: app-ui/block-and-unblock-a-user
  testWidgets('block then unblock a user from the members list', (tester) async {
    final core = group();
    await openApp(tester, core);
    await menu(tester, 'Members');

    await tap(tester, find.byKey(const ValueKey('moderate-bob')));
    expect(find.text('Report'), findsOneWidget);
    await tap(tester, find.text('Block'));
    expect(find.byKey(const Key('block-confirm')), findsOneWidget);
    await tap(tester, find.byKey(const Key('block-confirm-ok')));

    expect(core.calls, contains('blockUser'));
    // A blocked marker where the user appears.
    expect(find.textContaining('blocked'), findsOneWidget);

    await tap(tester, find.byKey(const ValueKey('moderate-bob')));
    await tap(tester, find.text('Unblock'));
    expect(core.calls, contains('unblockUser'));
    expect(find.textContaining('blocked'), findsNothing);
  });

  // spec: app-ui/report-a-message-or-a-user
  testWidgets('report a user from the members list', (tester) async {
    final core = group();
    await openApp(tester, core);
    await menu(tester, 'Members');

    await tap(tester, find.byKey(const ValueKey('moderate-bob')));
    await tap(tester, find.text('Report'));
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
