// app-shell: bottom navigation, Chats list, Conversation screen, Contacts tab,
// Profile tab.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/chat/chat_screen.dart';
import 'package:vyajena/core/core_service.dart';

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
  testWidgets('three tabs; the third is the round avatar, not a gear', (
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
    // The avatar is a circle.
    expect(
      find.descendant(
        of: find.byKey(const Key('profile-avatar')),
        matching: find.byType(CircleAvatar),
      ),
      findsOneWidget,
    );
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
        .map((t) => (t.title as Text).data)
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
  testWidgets('a closed DM is greyed at the bottom with a swipe-to-remove '
      'affordance that does nothing yet', (tester) async {
    final core = aliceInGroup()
      ..groupChats = [g.group(100, 'Weekend')]
      ..dmChats = [dmWithBob(closed: true)];
    await openHome(tester, core);
    final rows = tester
        .widgetList<ListTile>(find.byType(ListTile))
        .map((t) => (t.title as Text).data)
        .toList();
    expect(rows.last, 'Bobby (bob)');
    final dismiss = find.byKey(const ValueKey('dismiss-1'));
    expect(dismiss, findsOneWidget);
    await tester.drag(dismiss, const Offset(-600, 0));
    await settle(tester);
    // Not removed: delete-messages owns the removal.
    expect(find.byKey(const ValueKey('chat-1')), findsOneWidget);
    expect(find.text('Removing a closed chat is coming soon.'), findsOneWidget);
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
  testWidgets('tapping a member opens their DM', (tester) async {
    final core = aliceInGroup();
    await openHome(tester, core);
    await tap(tester, contactsTab);
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
