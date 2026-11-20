// app-ui: Chat list; Open a DM; DM screen (change `dms`).
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/chat/chat_screen.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

/// Alice's device in a group of akhil, alice and bob, where bob is "Bobby".
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
  }
  ..profiles = {
    'alice@chat': const ProfileInfo(displayName: 'Ali'),
    'bob@chat': const ProfileInfo(displayName: 'Bobby'),
  };

/// The DM with Bob, as chat 1.
ChatSummary dmWithBob({
  int unread = 0,
  bool waiting = false,
  bool closed = false,
}) => ChatSummary(
  chatId: 1,
  kind: ChatKindInfo.dm(peer: bob, waiting: waiting, closed: closed),
  unread: unread,
  unseenMentions: 0,
);

final composer = find.byKey(const Key('composer'));

void main() {
  // spec: app-ui/chat-list
  testWidgets('Two chats: the group first, then "Bobby (bob)" with an unread '
      'mark; tapping it opens the DM', (tester) async {
    final core = aliceInGroup()..dmChats = [dmWithBob(unread: 1)];
    core.receive(bob, 'hi from the DM', chatId: 1, emitEvent: false);
    await openApp(tester, core);

    final group = find.byKey(const ValueKey('chat-main'));
    final dm = find.byKey(const ValueKey('chat-1'));
    expect(group, findsOneWidget);
    expect(dm, findsOneWidget);
    expect(
      tester.getTopLeft(group).dy,
      lessThan(tester.getTopLeft(dm).dy),
      reason: 'the group comes first',
    );
    expect(find.text('lobby'), findsOneWidget);
    expect(find.text('Bobby (bob)'), findsOneWidget);
    expect(find.byKey(const ValueKey('chat-unread-1')), findsOneWidget);

    await tap(tester, dm);
    expect(find.text('hi from the DM'), findsOneWidget);
    expect(core.calls, contains('markChatRead'));
    expect(core.args['markChatRead'], 1);
  });

  // spec: app-ui/chat-list
  testWidgets('A DM that arrives while a chat is open does not leave it; the '
      'list shows its unread mark on return', (tester) async {
    final core = aliceInGroup();
    await openApp(tester, core); // lands in the group's chat over the list
    expect(composer, findsOneWidget, reason: 'in the group');

    core.dmChats = [dmWithBob(unread: 1)];
    core.receive(bob, 'psst', chatId: 1);
    await settle(tester);
    expect(composer, findsOneWidget, reason: 'still in the group');
    expect(find.text('psst'), findsNothing, reason: 'not the group chat');

    // Back to the chat list: the DM is there with its unread mark.
    await tester.pageBack();
    await settle(tester);
    expect(find.byKey(const ValueKey('chat-1')), findsOneWidget);
    expect(find.byKey(const ValueKey('chat-unread-1')), findsOneWidget);
  });

  // spec: app-ui/open-a-dm
  testWidgets('First message to Bob: "Message" on Bob opens the empty DM at '
      'once, which says it waits for his devices', (tester) async {
    final core = aliceInGroup();
    await openApp(tester, core);
    await menu(tester, 'Members');

    // No "Message" on Alice herself.
    expect(find.byKey(const ValueKey('message-alice')), findsNothing);
    await tap(tester, find.byKey(const ValueKey('message-bob')));
    expect(core.calls, contains('openDm'));
    expect(core.args['openDm'], bob);
    expect(find.text('Bobby'), findsWidgets, reason: 'the DM is titled Bobby');
    expect(find.byKey(const Key('dm-waiting')), findsOneWidget);
    expect(find.text(dmWaitingText('Bobby')), findsOneWidget);

    await tester.enterText(composer, 'hi bob');
    await tap(tester, find.byTooltip('Send'));
    final sent =
        core.args['sendMessageIn']!
            as ({int chatId, String text, List<MentionSpan> mentions});
    expect(sent.chatId, 1);
    expect(sent.text, 'hi bob');
    expect(find.text('hi bob'), findsOneWidget);
  });

  // spec: app-ui/open-a-dm
  testWidgets('Old server: no "Message" action', (tester) async {
    final core = aliceInGroup()..dms = false;
    await openApp(tester, core);
    await menu(tester, 'Members');
    expect(find.text('Message'), findsNothing);
  });

  // spec: app-ui/dm-screen
  testWidgets('Picker in a DM: "@" offers Bob only, no everyone', (
    tester,
  ) async {
    final core = aliceInGroup()
      ..admin = true
      ..dmChats = [dmWithBob()];
    await openApp(tester, core);
    await tap(tester, find.byKey(const ValueKey('chat-1')));

    await tester.enterText(composer, '@');
    await settle(tester);
    final picker = find.byKey(const Key('mention-picker'));
    expect(picker, findsOneWidget);
    expect(
      find.descendant(of: picker, matching: find.text('Bobby (bob)')),
      findsOneWidget,
    );
    expect(
      find.descendant(of: picker, matching: find.text('akhil')),
      findsNothing,
    );
    expect(
      find.descendant(of: picker, matching: find.text('everyone')),
      findsNothing,
    );
    // No admin actions in the DM's menu, even for the admin.
    await tap(tester, find.byTooltip('Menu'));
    expect(find.text('Invite a user'), findsNothing);
  });

  // spec: app-ui/dm-screen
  testWidgets('Closed DM: its messages, the banner, and no composer', (
    tester,
  ) async {
    final core = aliceInGroup()..dmChats = [dmWithBob(closed: true)];
    core.receive(bob, 'last words', chatId: 1, emitEvent: false);
    await openApp(tester, core);
    expect(find.text(dmClosedText('Bobby')), findsOneWidget);
    await tap(tester, find.byKey(const ValueKey('chat-1')));

    expect(find.text('last words'), findsOneWidget);
    expect(find.byKey(const Key('dm-closed')), findsOneWidget);
    expect(find.text(dmClosedText('Bobby')), findsOneWidget);
    expect(composer, findsNothing);
  });
}
