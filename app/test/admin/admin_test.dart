// app-ui: Admin screens.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

FakeCoreService adminCore() => FakeCoreService(storedUser: akhil)..admin = true;

const adminEntries = [
  'Invite a user',
  'Invites',
  'Remove a user',
  'Group name',
];

void main() {
  // spec: app-ui/admin-screens
  testWidgets('Admin invites Bob: the core calls Admin.CreateInvite and the '
      'app shows the invite text with a share button', (tester) async {
    final shared = recordShares(tester);
    final core = await openApp(tester, adminCore());
    await menu(tester, 'Invite a user');
    await tester.enterText(find.byKey(const Key('username')), 'bob');
    await tap(tester, find.widgetWithText(FilledButton, 'Invite'));

    expect(core.calls, contains('createInvite'));
    expect(core.args['createInvite'], 'bob');
    expect(find.text('vyajena-invite:bob'), findsOneWidget);
    await tap(tester, find.widgetWithText(FilledButton, 'Share'));
    expect(shared, ['vyajena-invite:bob']);
  });

  // spec: app-ui/admin-screens
  testWidgets('Bob has no admin screens: no invite, remove or group-settings '
      'entries', (tester) async {
    await openApp(tester, FakeCoreService(storedUser: bob));
    await tap(tester, find.byTooltip('Menu'));
    for (final entry in adminEntries) {
      expect(find.text(entry), findsNothing);
    }
    expect(find.text('Members'), findsOneWidget);
    expect(find.text('Profile'), findsOneWidget);
  });

  // spec: app-ui/admin-screens
  testWidgets('the admin\'s device shows every admin entry', (tester) async {
    await openApp(tester, adminCore());
    await tap(tester, find.byTooltip('Menu'));
    for (final entry in adminEntries) {
      expect(find.text(entry), findsOneWidget);
    }
  });

  // spec: app-ui/admin-screens
  testWidgets('on the admin\'s first start the app creates the group', (
    tester,
  ) async {
    final core = adminCore()..epochValue = null;
    await openApp(tester, core);
    expect(find.byKey(const Key('composer')), findsNothing);
    await tap(tester, find.text('Create the group'));
    expect(core.calls, contains('createGroup'));
    // The group now exists; the app lands on the chat list, so open the chat.
    await enterGroup(tester);
    expect(find.byKey(const Key('composer')), findsOneWidget);
  });

  // spec: app-ui/admin-screens
  testWidgets('the admin lists the unused invites and revokes one', (
    tester,
  ) async {
    final core = adminCore()
      ..invites = const [
        InviteInfo(code: 'c-bob', user: bob, expiresAtMs: 0),
        InviteInfo(
          code: 'c-carol',
          user: UserRef(username: 'carol', serverId: 'chat'),
          expiresAtMs: 0,
        ),
      ];
    await openApp(tester, core);
    await menu(tester, 'Invites');
    expect(find.text('bob@chat'), findsOneWidget);
    expect(find.text('carol@chat'), findsOneWidget);

    final row = find.ancestor(
      of: find.text('bob@chat'),
      matching: find.byType(ListTile),
    );
    await tap(tester, find.descendant(of: row, matching: find.text('Revoke')));
    await tap(tester, find.widgetWithText(FilledButton, 'Revoke'));
    expect(core.args['revokeInvite'], 'c-bob');
    expect(find.text('bob@chat'), findsNothing);
    expect(find.text('carol@chat'), findsOneWidget);
  });

  // spec: app-ui/admin-screens
  testWidgets('the admin removes a user', (tester) async {
    final core = adminCore()
      ..memberDevices = const [
        MemberDevice(user: akhil, device: '01'),
        MemberDevice(user: bob, device: '03'),
      ];
    await openApp(tester, core);
    await menu(tester, 'Remove a user');
    // Not the admin themself.
    expect(find.text('akhil'), findsNothing);
    await tap(tester, find.widgetWithText(TextButton, 'Remove'));
    await tap(tester, find.widgetWithText(FilledButton, 'Remove'));
    expect(core.args['removeUser'], bob);
    expect(find.text('Nobody else is in the group'), findsOneWidget);
  });

  // spec: app-ui/admin-screens
  testWidgets('the admin changes the group name', (tester) async {
    final core = adminCore();
    await openApp(tester, core);
    await menu(tester, 'Group name');
    await tester.enterText(find.byKey(const Key('groupName')), 'Family');
    await tap(tester, find.text('Save'));
    expect(core.args['setGroupName'], 'Family');

    // The settings reach the members encrypted; the chat's title follows.
    core.emit(const CoreEvent.groupSettingsChanged(name: 'Family', by: akhil));
    await settle(tester);
    expect(find.text('Family'), findsOneWidget);
  });
}
