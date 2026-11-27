// app-shell: bottom navigation, Chats list, Conversation screen, Contacts tab,
// Profile tab.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/chat/chat_screen.dart';
import 'package:vyajena/core/core_service.dart';
import 'package:vyajena/members/contacts_tab.dart';
import 'package:vyajena/shell/avatars.dart';

import '../chat/dms_test.dart' show aliceInGroup, dmWithBob;
import '../chat/groups_test.dart' as g;
import '../support/app.dart';
import '../support/fake_core_service.dart';

final chatsTab = find.byKey(const Key('tab-chats'));
final contactsTab = find.byKey(const Key('tab-contacts'));
final profileTab = find.byKey(const Key('tab-profile'));

/// Alice in a lobby, a "Weekend" group and a DM with Bob.
FakeCoreService aliceWithEverything() {
  final core = aliceInGroup()
    ..groupChats = [g.group(100, 'Weekend', unread: 2)]
    ..groupMemberDevices = {
      100: const [
        MemberDevice(user: akhil, device: '01'),
        MemberDevice(user: alice, device: '02'),
        MemberDevice(user: bob, device: '03'),
      ],
    }
    ..dmChats = [dmWithBob(unread: 1)];
  return core;
}

/// A tall screen, so the whole Profile tab is built and on screen.
void tall(WidgetTester tester) {
  tester.view.physicalSize = const Size(800, 2400);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
}

void main() {
  // spec: app-shell/bottom-navigation
  testWidgets('three tabs; the third is the square avatar, not a gear', (
    tester,
  ) async {
    await openHome(tester, aliceWithEverything());
    expect(find.byKey(const Key('bottom-nav')), findsOneWidget);
    expect(find.text('Chats'), findsWidgets);
    expect(find.text('Contacts'), findsWidgets);
    expect(find.text('Profile'), findsWidgets);
    expect(
      find.descendant(
        of: profileTab,
        matching: find.byKey(const Key('profile-avatar')),
      ),
      findsOneWidget,
    );
    expect(find.byIcon(Icons.settings), findsNothing);
    expect(find.byIcon(Icons.settings_outlined), findsNothing);
    // The avatar is a square (Arcade look), the monogram box.
    expect(
      find.descendant(of: profileTab, matching: find.byType(NameAvatar)),
      findsOneWidget,
    );
  });

  // spec: presence/toggle-your-status-on-the-profile-screen
  testWidgets('the Profile avatar carries this user\'s MANUAL presence dot: '
      'red offline (the default), green online, driven by the chosen flag not '
      'the connection', (tester) async {
    final core = aliceWithEverything();
    await openHome(tester, core);
    final scheme = Theme.of(
      tester.element(find.byKey(const Key('bottom-nav'))),
    ).colorScheme;
    final dot = find.descendant(
      of: profileTab,
      matching: find.byKey(const Key('profile-presence-dot')),
    );
    ShapeDecoration deco() =>
        tester.widget<Container>(dot).decoration! as ShapeDecoration;

    // Offline is the first-launch default, so the dot starts in the error
    // colour.
    expect(dot, findsOneWidget);
    expect(deco().color, scheme.error);

    // Connecting does NOT turn it green: presence is manual, not derived from
    // the connection.
    core.emit(const CoreEvent.connection(status: ConnectionStatus.connected));
    await settle(tester);
    expect(deco().color, scheme.error);

    // The user manually going Online turns it green.
    await core.setMyPresence(true);
    await settle(tester);
    expect(deco().color, const Color(0xFF34C759));

    // The user manually going Offline turns it red again.
    await core.setMyPresence(false);
    await settle(tester);
    expect(deco().color, scheme.error);

    // No peer presence is known in this fixture, so only the Profile avatar
    // has a dot (the chat-list and contact avatars carry none).
    expect(find.byKey(const Key('profile-presence-dot')), findsOneWidget);
    expect(find.byKey(const Key('presence-dot')), findsNothing);
  });

  // spec: app-shell/bottom-navigation
  testWidgets('tapping the avatar tab opens profile, settings and server info; '
      'tab state survives switching', (tester) async {
    tall(tester);
    await openHome(tester, aliceWithEverything());
    await tap(tester, profileTab);
    expect(find.byKey(const Key('server-id')), findsOneWidget);
    expect(find.byKey(const Key('settings-link-device')), findsOneWidget);
    // Type a display name, leave the tab and come back: still typed.
    await tester.enterText(find.byKey(const Key('displayName')), 'Alicia');
    await tap(tester, contactsTab);
    expect(find.byKey(const Key('contact-bob')), findsOneWidget);
    await tap(tester, profileTab);
    expect(find.text('Alicia'), findsOneWidget);
    await tap(tester, chatsTab);
    expect(find.byKey(const ValueKey('chat-main')), findsOneWidget);
  });

  // spec: app-shell/chats-list
  testWidgets('lobby first, then the group, then the DM, each with a preview '
      'and unread count', (tester) async {
    final core = aliceWithEverything();
    core.receive(bob, 'see you at noon', chatId: 1, emitEvent: false);
    core.receive(akhil, 'hello all', chatId: 100, emitEvent: false);
    core.receiveAudio(bob, chatId: 1);
    await openHome(tester, core);
    final rows = tester
        .widgetList<ListTile>(find.byType(ListTile))
        .map(tileTitle)
        .toList();
    expect(rows, ['lobby', 'Weekend', 'Bobby (bob)']);
    expect(find.byKey(const ValueKey('chat-unread-100')), findsOneWidget);
    expect(find.byKey(const ValueKey('chat-unread-1')), findsOneWidget);
    // A group preview names the sender; a DM's does not. A voice message
    // shows the mic glyph.
    expect(find.text('akhil: hello all'), findsOneWidget);
    expect(find.text('Voice message'), findsOneWidget);
    expect(find.byIcon(Icons.mic_none), findsOneWidget);
  });

  // spec: app-shell/chats-list
  testWidgets('a picture shows the image glyph in the preview', (tester) async {
    final core = aliceWithEverything();
    core.receiveImage(bob, chatId: 1);
    await openHome(tester, core);
    expect(find.text('Photo'), findsOneWidget);
    expect(find.byIcon(Icons.image_outlined), findsOneWidget);
  });

  // spec: app-shell/chats-list
  testWidgets('a closed DM is greyed at the bottom of the list', (
    tester,
  ) async {
    final core = aliceInGroup()
      ..groupChats = [g.group(100, 'Weekend')]
      ..dmChats = [dmWithBob(closed: true)];
    await openHome(tester, core);
    final rows = tester
        .widgetList<ListTile>(find.byType(ListTile))
        .map(tileTitle)
        .toList();
    expect(rows.last, 'Bobby (bob)');
  });

  Future<void> swipeClosedDm(WidgetTester tester) async {
    await tester.drag(
      find.byKey(const ValueKey('dismiss-1')),
      const Offset(-600, 0),
    );
    await settle(tester);
  }

  FakeCoreService closedDmCore() => aliceInGroup()
    ..groupChats = [g.group(100, 'Weekend')]
    ..dmChats = [dmWithBob(closed: true)];

  // spec: app-ui/remove-a-closed-dm-from-the-list
  testWidgets('swiping a closed DM asks first; cancel keeps it', (
    tester,
  ) async {
    final core = closedDmCore();
    await openHome(tester, core);
    await swipeClosedDm(tester);
    // Never a bare swipe: nothing is removed before the user confirms.
    expect(find.byKey(const Key('remove-closed-dm-dialog')), findsOneWidget);
    expect(core.calls, isNot(contains('removeClosedDm')));
    await tap(tester, find.byKey(const Key('remove-closed-dm-cancel')));
    expect(find.byKey(const ValueKey('chat-1')), findsOneWidget);
    expect(core.calls, isNot(contains('removeClosedDm')));
  });

  // spec: app-ui/remove-a-closed-dm-from-the-list
  testWidgets('confirming removes the closed DM and its row', (tester) async {
    final core = closedDmCore();
    await openHome(tester, core);
    await swipeClosedDm(tester);
    await tap(tester, find.byKey(const Key('remove-closed-dm-confirm')));
    expect(core.args['removeClosedDm'], 1);
    expect(find.byKey(const ValueKey('chat-1')), findsNothing);
    // The group row is untouched.
    expect(find.byKey(const ValueKey('chat-100')), findsOneWidget);
    expect(find.text('Removing a closed chat is coming soon.'), findsNothing);
  });

  // spec: app-ui/remove-a-closed-dm-from-the-list
  testWidgets('a failed removal keeps the row and says why', (tester) async {
    final core = closedDmCore()..removeClosedDmError = StateError('boom');
    await openHome(tester, core);
    await swipeClosedDm(tester);
    await tap(tester, find.byKey(const Key('remove-closed-dm-confirm')));
    expect(find.byKey(const ValueKey('chat-1')), findsOneWidget);
    expect(find.textContaining('boom'), findsOneWidget);
  });

  // spec: app-ui/remove-a-closed-dm-from-the-list
  testWidgets('no Remove swipe on an open DM or a group', (tester) async {
    final core = aliceInGroup()
      ..groupChats = [g.group(100, 'Weekend')]
      ..dmChats = [dmWithBob()];
    await openHome(tester, core);
    expect(find.byKey(const ValueKey('dismiss-1')), findsNothing);
    expect(find.byKey(const ValueKey('dismiss-100')), findsNothing);
    expect(find.byKey(const ValueKey('dismiss-0')), findsNothing);
  });

  // spec: app-shell/conversation-screen
  testWidgets('sender name in a group, none in a DM', (tester) async {
    final core = aliceWithEverything();
    core.receive(akhil, 'in the group', chatId: 100, emitEvent: false);
    core.receive(bob, 'in the dm', chatId: 1, emitEvent: false);
    await openHome(tester, core);
    await tap(tester, find.byKey(const ValueKey('chat-100')));
    expect(find.text('in the group'), findsOneWidget);
    expect(find.text('akhil'), findsOneWidget);
    expect(find.byKey(const Key('conversation-avatar')), findsOneWidget);
    expect(find.byKey(const Key('conversation-subtitle')), findsOneWidget);
    await tester.pageBack();
    await settle(tester);
    await tap(tester, find.byKey(const ValueKey('chat-1')));
    expect(find.text('in the dm'), findsOneWidget);
    expect(find.text('Bobby (bob)'), findsNothing);
    expect(find.text('bob'), findsOneWidget); // the app bar subtitle only
  });

  // spec: app-shell/conversation-screen
  testWidgets('the attachment control opens a sheet offering a photo', (
    tester,
  ) async {
    final core = aliceWithEverything();
    await openHome(tester, core);
    await tap(tester, find.byKey(const ValueKey('chat-1')));
    await tap(tester, find.byKey(const Key('attach-image')));
    expect(find.byKey(const Key('attachment-sheet')), findsOneWidget);
    expect(find.text('Photo'), findsOneWidget);
    // The mic stays beside the field while nothing is typed.
    await tester.tapAt(const Offset(5, 5));
    await settle(tester);
    expect(find.byKey(const Key('record-voice')), findsOneWidget);
    expect(find.byType(ChatScreen), findsOneWidget);
  });

  // spec: app-shell/contacts-tab
  testWidgets('members carry the "you" marker, the admin the badge', (
    tester,
  ) async {
    final core = g.adminWithGroups()..storedUser = akhil;
    await openHome(tester, core);
    await tap(tester, contactsTab);
    expect(find.byKey(const Key('you-akhil')), findsOneWidget);
    expect(find.byKey(const Key('admin-badge-akhil')), findsOneWidget);
    expect(find.byKey(const Key('you-alice')), findsNothing);
    expect(find.byKey(const Key('admin-badge-alice')), findsNothing);
    expect(find.byKey(const Key('contact-bob')), findsOneWidget);
  });

  // spec: app-shell/contacts-tab
  testWidgets('a non-admin member sees the admin badge on the admin row', (
    tester,
  ) async {
    // Alice is not the admin; the group names akhil as its admin.
    final core = aliceInGroup()
      ..adminUserRef = akhil
      ..memberDevices = const [
        MemberDevice(user: akhil, device: '01'),
        MemberDevice(user: alice, device: '02'),
        MemberDevice(user: bob, device: '03'),
      ];
    await openHome(tester, core);
    await tap(tester, contactsTab);
    expect(find.byKey(const Key('admin-badge-akhil')), findsOneWidget);
    expect(find.byKey(const Key('you-alice')), findsOneWidget);
    expect(find.byKey(const Key('admin-badge-alice')), findsNothing);
    expect(find.byKey(const Key('admin-badge-bob')), findsNothing);
  });

  // spec: app-shell/contacts-tab
  testWidgets('tapping a member opens their DM', (tester) async {
    final core = aliceInGroup();
    await openHome(tester, core);
    await tap(tester, contactsTab);
    await tap(tester, find.byKey(const Key('contact-bob')));
    expect(core.calls, contains('openDm'));
    expect(find.byType(ChatScreen), findsOneWidget);
  });

  // spec: app-ui/contact-tap
  testWidgets('tapping your own row opens the Profile tab', (tester) async {
    tall(tester);
    final core = aliceInGroup();
    await openHome(tester, core);
    await tap(tester, contactsTab);
    await tap(tester, find.byKey(const Key('contact-alice')));
    expect(find.byKey(const Key('profile-list')), findsOneWidget);
    expect(core.calls, isNot(contains('openDm')));
  });

  // spec: app-ui/contact-tap
  testWidgets('a tap when DMs are not offered says so', (tester) async {
    final core = aliceInGroup()..dms = false;
    await openHome(tester, core);
    await tap(tester, contactsTab);
    await tap(tester, find.byKey(const Key('contact-bob')));
    expect(find.text(dmsUnavailableText), findsOneWidget);
    expect(core.calls, isNot(contains('openDm')));
  });

  // spec: app-ui/contact-tap
  testWidgets('DMs offered after the tab loaded: the tap still opens the DM', (
    tester,
  ) async {
    final core = aliceInGroup()..dms = false;
    await openHome(tester, core);
    await tap(tester, contactsTab);
    // The device connects and learns the server offers DMs.
    core.dms = true;
    await tap(tester, find.byKey(const Key('contact-bob')));
    expect(core.calls, contains('openDm'));
    expect(find.byType(ChatScreen), findsOneWidget);
  });

  // spec: app-shell/contacts-tab
  testWidgets('admin entry points show for the admin and not for a member', (
    tester,
  ) async {
    final admin = g.adminWithGroups()..storedUser = akhil;
    await openHome(tester, admin);
    await tap(tester, contactsTab);
    expect(find.byKey(const Key('contacts-invite')), findsOneWidget);
    expect(find.byKey(const Key('contacts-new-group')), findsOneWidget);
  });

  // spec: app-shell/contacts-tab
  testWidgets('a non-admin sees no invite or group actions', (tester) async {
    await openHome(tester, aliceInGroup());
    await tap(tester, contactsTab);
    expect(find.byKey(const Key('contacts-invite')), findsNothing);
    expect(find.byKey(const Key('contacts-new-group')), findsNothing);
    expect(find.byKey(const Key('contacts-remove-user')), findsNothing);
  });

  // spec: app-shell/profile-tab
  testWidgets('editing the display name goes through the core and stays on '
      'the tab', (tester) async {
    tall(tester);
    final core = aliceInGroup();
    await openHome(tester, core);
    await tap(tester, profileTab);
    await tester.enterText(find.byKey(const Key('displayName')), 'Alicia');
    await tap(tester, find.widgetWithText(FilledButton, 'Save'));
    expect(core.calls, contains('setProfile'));
    // Still the Profile tab, not closed.
    expect(find.byKey(const Key('displayName')), findsOneWidget);
  });

  // spec: app-shell/profile-tab
  testWidgets('saving on the profile tab drops the keyboard so Chats is '
      'reachable again', (tester) async {
    tall(tester);
    final core = aliceInGroup();
    await openHome(tester, core);
    await tap(tester, profileTab);
    await tester.enterText(find.byKey(const Key('about')), 'tea');
    // The keyboard is up; on the Profile tab it covers the bottom navigation.
    expect(tester.testTextInput.isVisible, isTrue);
    await tap(tester, find.widgetWithText(FilledButton, 'Save'));
    // After the save the keyboard is gone, so the user is not trapped and can
    // get back to Chats.
    expect(tester.testTextInput.isVisible, isFalse);
    await tap(tester, chatsTab);
    expect(find.byKey(const ValueKey('chat-main')), findsOneWidget);
  });

  // spec: app-shell/profile-tab
  testWidgets('tapping outside a field on the profile tab dismisses the '
      'keyboard', (tester) async {
    tall(tester);
    await openHome(tester, aliceInGroup());
    await tap(tester, profileTab);
    await tester.enterText(find.byKey(const Key('displayName')), 'Alicia');
    expect(tester.testTextInput.isVisible, isTrue);
    // A tap on the tab's body (not on a field) drops the keyboard.
    await tap(tester, find.byKey(const Key('profile-header-avatar')));
    expect(tester.testTextInput.isVisible, isFalse);
  });

  // spec: app-shell/profile-tab
  testWidgets('the server id and the connection state show', (tester) async {
    tall(tester);
    final core = aliceInGroup();
    await openHome(tester, core);
    await tap(tester, profileTab);
    expect(
      find.descendant(
        of: find.byKey(const Key('server-id')),
        matching: find.text('chat'),
      ),
      findsOneWidget,
    );
    core.emit(const CoreEvent.connection(status: ConnectionStatus.connected));
    await settle(tester);
    expect(
      find.descendant(
        of: find.byKey(const Key('server-connection')),
        matching: find.text('Connected'),
      ),
      findsOneWidget,
    );
  });
}
