// app-ui: Admin screens.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

FakeCoreService adminCore() => FakeCoreService(storedUser: akhil)..admin = true;

const adminEntries = ['Invite a user', 'Invites', 'Remove a user'];

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
  testWidgets('inviting a username that already has an unused invite says so', (
    tester,
  ) async {
    final core = adminCore()
      ..invites = const [InviteInfo(code: 'c-bob', user: bob, expiresAtMs: 0)];
    await openApp(tester, core);
    await menu(tester, 'Invite a user');
    await tester.enterText(find.byKey(const Key('username')), 'bob');
    await tap(tester, find.widgetWithText(FilledButton, 'Invite'));
    expect(find.byKey(const Key('invite-existed')), findsOneWidget);
  });

  // spec: app-ui/admin-screens
  testWidgets('inviting a fresh username shows no already-existed note', (
    tester,
  ) async {
    await openApp(tester, adminCore());
    await menu(tester, 'Invite a user');
    await tester.enterText(find.byKey(const Key('username')), 'carol');
    await tap(tester, find.widgetWithText(FilledButton, 'Invite'));
    expect(find.byKey(const Key('invite-existed')), findsNothing);
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

  // spec: app-ui/the-lobby-appears-without-a-manual-create-step
  testWidgets('the admin\'s first device creates the lobby with no manual step', (
    tester,
  ) async {
    final core = adminCore()..epochValue = null;
    await openHome(tester, core);
    await settle(tester);
    // No "Create the group" button: the app calls createGroup itself.
    expect(find.text('Create the group'), findsNothing);
    expect(core.calls, contains('createGroup'));
    // The lobby now exists, so its row is on the chat list; opening it shows
    // the composer.
    expect(find.byKey(const ValueKey('chat-main')), findsOneWidget);
    await enterGroup(tester);
    expect(find.byKey(const Key('composer')), findsOneWidget);
  });

  // spec: app-ui/the-lobby-appears-without-a-manual-create-step
  testWidgets('a non-admin device never creates a lobby', (tester) async {
    final core = FakeCoreService(storedUser: bob)..epochValue = null;
    await openApp(tester, core);
    // It waits to be added; it must not try to create the lobby.
    expect(core.calls, isNot(contains('createGroup')));
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

  FakeCoreService adminWithBob() =>
      adminCore()
        ..memberDevices = const [
          MemberDevice(user: akhil, device: '01'),
          MemberDevice(user: bob, device: '03'),
        ];

  // spec: app-ui/re-invite-action
  testWidgets('the admin re-invites a member who lost their phone: the confirm '
      'warns and calls nothing until confirmed', (tester) async {
    final core = adminWithBob();
    await openApp(tester, core);
    await menu(tester, 'Remove a user');
    await tap(tester, find.byKey(const ValueKey('reinvite-bob')));
    // The confirm names Bob and warns it resets his devices and loses history.
    expect(find.text('Re-invite bob?'), findsOneWidget);
    expect(find.textContaining('resets their devices'), findsOneWidget);
    expect(find.textContaining('lose'), findsOneWidget);
    // Nothing is minted before the admin confirms.
    expect(core.calls, isNot(contains('createInvite')));
  });

  // spec: app-ui/re-invite-action
  testWidgets('confirming the re-invite calls CreateInvite and shows a '
      'shareable code', (tester) async {
    final shared = recordShares(tester);
    final core = adminWithBob();
    await openApp(tester, core);
    await menu(tester, 'Remove a user');
    await tap(tester, find.byKey(const ValueKey('reinvite-bob')));
    await tap(tester, find.widgetWithText(FilledButton, 'Re-invite'));
    // CreateInvite is called for the same username, with no remove first.
    expect(core.calls, contains('createInvite'));
    expect(core.args['createInvite'], 'bob');
    expect(core.calls, isNot(contains('removeUser')));
    // The shareable invite text is shown with a Share button.
    expect(find.text('vyajena-invite:bob'), findsOneWidget);
    await tap(tester, find.widgetWithText(FilledButton, 'Share'));
    expect(shared, ['vyajena-invite:bob']);
  });

  // spec: app-ui/re-invite-action
  testWidgets('cancelling the re-invite changes nothing', (tester) async {
    final core = adminWithBob();
    await openApp(tester, core);
    await menu(tester, 'Remove a user');
    await tap(tester, find.byKey(const ValueKey('reinvite-bob')));
    await tap(tester, find.widgetWithText(TextButton, 'Cancel'));
    expect(core.calls, isNot(contains('createInvite')));
    expect(core.calls, isNot(contains('removeUser')));
    // Bob is still listed, untouched.
    expect(find.text('bob'), findsOneWidget);
  });

  // spec: app-ui/re-invite-action
  testWidgets('a non-admin sees no re-invite action', (tester) async {
    final core = FakeCoreService(storedUser: bob)
      ..memberDevices = const [
        MemberDevice(user: akhil, device: '01'),
        MemberDevice(user: bob, device: '03'),
      ];
    await openApp(tester, core);
    // No admin members screen to reach, so no Re-invite anywhere.
    await tap(tester, find.byTooltip('Menu'));
    expect(find.text('Remove a user'), findsNothing);
    expect(find.text('Re-invite'), findsNothing);
  });
}
