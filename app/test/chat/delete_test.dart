// app-ui: Deleting a message (change `delete-messages`): the long-press Delete
// action (admin-only in a group, own-only in a DM), the tombstone, and the DM
// Reveal affordance.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/chat/message_tile.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

FakeCoreService group() => FakeCoreService(storedUser: alice)
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

ChatSummary dmWithBob() => const ChatSummary(
  chatId: 1,
  kind: ChatKindInfo.dm(peer: bob, waiting: false, closed: false),
  unread: 0,
  unseenMentions: 0,
);

Finder tile(String text) =>
    find.ancestor(of: find.text(text), matching: find.byType(MessageTile));

Future<int> sendIn(WidgetTester tester, FakeCoreService core, String text) async {
  await tester.enterText(find.byKey(const Key('composer')), text);
  await tap(tester, find.byTooltip('Send'));
  return core.messages.last.id;
}

void main() {
  // spec: app-ui/deleting-a-message
  testWidgets('the admin deletes a member message and it reads "by the admin"', (
    tester,
  ) async {
    final core = group()..admin = true;
    await openApp(tester, core);
    core.receive(bob, 'hello');
    await settle(tester);

    // The admin's long-press offers Delete.
    await tester.longPress(find.text('hello'));
    await settle(tester);
    expect(find.text(deleteLabel), findsOneWidget);

    core.deleteByAdmin = true;
    await tap(tester, find.text(deleteLabel));
    await settle(tester);

    expect(find.text('hello'), findsNothing);
    expect(find.text(deletedByAdminText), findsOneWidget);
    expect(core.calls, contains('deleteMessage'));
  });

  // spec: app-ui/deleting-a-message
  testWidgets('a non-admin member sees no Delete in a group', (tester) async {
    final core = group()..admin = false;
    await openApp(tester, core);
    final id = await sendIn(tester, core, 'mine');
    expect(id, greaterThan(0));

    await tester.longPress(find.text('mine'));
    await settle(tester);
    expect(find.text(replyLabel), findsOneWidget);
    expect(find.text(deleteLabel), findsNothing);
  });

  // spec: app-ui/deleting-a-message
  testWidgets('deleting an own DM message leaves a reveal-able tombstone', (
    tester,
  ) async {
    final core = group()
      ..admin = false
      ..dmChats = [dmWithBob()];
    await openHome(tester, core);
    await tap(tester, find.byKey(const ValueKey('chat-1')));

    final id = await sendIn(tester, core, 'secret');
    expect(id, greaterThan(0));

    // The sender may delete their own DM message.
    await tester.longPress(find.text('secret'));
    await settle(tester);
    expect(find.text(deleteLabel), findsOneWidget);
    await tap(tester, find.text(deleteLabel));
    await settle(tester);

    expect(find.text('secret'), findsNothing);
    expect(find.text(deletedText), findsOneWidget);

    // Reveal shows the original back.
    expect(find.byKey(const Key('reveal')), findsOneWidget);
    await tap(tester, find.byKey(const Key('reveal')));
    await settle(tester);
    expect(find.text('secret'), findsOneWidget);
    expect(core.calls, contains('reveal'));
  });
}
