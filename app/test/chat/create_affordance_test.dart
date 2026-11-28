// app-ui: Chat list; Create affordance (change `create-affordance`).
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

/// Alice's device in a group of akhil, alice and bob (bob is "Bobby").
FakeCoreService aliceInGroup() => FakeCoreService(storedUser: alice)
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

final composer = find.byKey(const Key('composer'));
final fab = find.byKey(const Key('new-chat-fab'));

/// Taps the `+` without `ensureVisible` (it is not in a scrollable), after
/// flushing any route-transition frames so the tap is not absorbed.
Future<void> tapFab(WidgetTester tester) async {
  // Not `pumpAndSettle`: the app bar's connecting indicator pulses forever
  // (connect-feedback), so a finite settle is the only safe flush.
  await settle(tester);
  await tester.tap(fab);
  await settle(tester);
}

void main() {
  // spec: app-ui/chat-list
  testWidgets('Opens on the chat list even with no DM: the group is the first '
      'row and the group composer is not shown', (tester) async {
    final core = aliceInGroup();
    await openHome(tester, core);

    expect(find.byKey(const ValueKey('chat-main')), findsOneWidget);
    expect(find.text('#lobby'), findsOneWidget);
    expect(composer, findsNothing, reason: 'the list, not the group chat');
  });

  // spec: app-ui/create-affordance
  testWidgets('New DM: + then New DM lets Alice pick Bob (not herself) and '
      'opens the DM through openDm', (tester) async {
    final core = aliceInGroup();
    await openHome(tester, core);

    await tapFab(tester);
    await tap(tester, find.byKey(const Key('new-dm')));

    // The picker offers the other members, not Alice herself.
    expect(find.byKey(const ValueKey('pick-alice')), findsNothing);
    expect(find.byKey(const ValueKey('pick-akhil')), findsOneWidget);
    await tap(tester, find.byKey(const ValueKey('pick-bob')));

    expect(core.calls, contains('openDm'));
    expect(core.args['openDm'], bob);
    // The DM opened at once, waiting for Bob's devices.
    expect(find.byKey(const Key('dm-waiting')), findsOneWidget);
    expect(composer, findsOneWidget);
  });

  // spec: app-ui/create-affordance
  testWidgets('New group only for the admin: a non-admin sees New DM but no '
      'New group', (tester) async {
    final member = aliceInGroup()..admin = false;
    await openHome(tester, member);
    await tapFab(tester);
    expect(find.byKey(const Key('new-dm')), findsOneWidget);
    expect(find.byKey(const Key('new-group')), findsNothing);
  });

  // spec: app-ui/create-affordance
  testWidgets('New group only for the admin: the admin sees both New DM and '
      'New group', (tester) async {
    final admin = aliceInGroup()..admin = true;
    await openHome(tester, admin);
    await tapFab(tester);
    expect(find.byKey(const Key('new-dm')), findsOneWidget);
    expect(find.byKey(const Key('new-group')), findsOneWidget);
  });

  // spec: app-ui/create-affordance
  testWidgets('Admin creates a group: + then New group opens the member '
      'picker', (tester) async {
    final core = aliceInGroup()..admin = true;
    await openHome(tester, core);

    await tapFab(tester);
    await tap(tester, find.byKey(const Key('new-group')));
    expect(find.byKey(const Key('create-group-confirm')), findsOneWidget);
    expect(core.calls, isNot(contains('createGroup')));
  });

  // spec: app-ui/create-affordance
  testWidgets('New group is not offered when the server lacks version 3', (
    tester,
  ) async {
    final core = aliceInGroup()
      ..admin = true
      ..namedGroups = false;
    await openHome(tester, core);
    await tapFab(tester);
    expect(find.byKey(const Key('new-group')), findsNothing);
  });

  // spec: app-ui/create-affordance
  testWidgets('No + button for a non-admin whose server does not offer DMs', (
    tester,
  ) async {
    final core = aliceInGroup()
      ..admin = false
      ..dms = false;
    await openHome(tester, core);
    expect(fab, findsNothing);
  });
}
