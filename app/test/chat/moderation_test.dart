// app-ui: Block and unblock a user; Report a message or a user; The admin sees
// reports (change `report-and-block`). The message long-press Report/Block
// actions, the pre-block confirm, the report reason prompt, and the admin's
// distinct report presentation with a path to remove-user.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/admin/remove_user_screen.dart';
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
  muted: false,
);

void main() {
  // spec: app-ui/block-and-unblock-a-user
  testWidgets('blocking from a message confirms, then hides the sender', (
    tester,
  ) async {
    final core = group();
    await openApp(tester, core);
    core.receive(bob, 'be nasty');
    await settle(tester);
    expect(find.text('be nasty'), findsOneWidget);

    await tester.longPress(find.text('be nasty'));
    await settle(tester);
    // Report and Block are offered together on an abusive message.
    expect(find.text(blockLabel), findsOneWidget);
    expect(find.text(reportLabel), findsOneWidget);

    await tap(tester, find.text(blockLabel));
    // The confirm warns it hides on this device only.
    expect(find.byKey(const Key('block-confirm')), findsOneWidget);
    expect(find.textContaining('this device only'), findsOneWidget);
    await tap(tester, find.byKey(const Key('block-confirm-ok')));

    expect(core.calls, contains('blockUser'));
    expect(find.text('be nasty'), findsNothing);
  });

  // spec: app-ui/report-a-message-or-a-user
  testWidgets('reporting a message asks a reason and confirms it was sent', (
    tester,
  ) async {
    final core = group();
    await openApp(tester, core);
    core.receive(bob, 'spam spam');
    await settle(tester);

    await tester.longPress(find.text('spam spam'));
    await settle(tester);
    await tap(tester, find.text(reportLabel));

    expect(find.byKey(const Key('report-reason')), findsOneWidget);
    await tester.enterText(
      find.byKey(const Key('report-reason-field')),
      'spamming',
    );
    await tap(tester, find.byKey(const Key('report-reason-send')));

    expect(core.calls, contains('reportMessage'));
    expect(core.lastReport?.reported, bob);
    expect(find.text('Report sent'), findsOneWidget);
  });

  // spec: app-ui/report-a-message-or-a-user
  testWidgets('an own message offers neither Report nor Block', (tester) async {
    final core = group();
    await openApp(tester, core);
    await tester.enterText(find.byKey(const Key('composer')), 'my words');
    await tap(tester, find.byTooltip('Send'));

    await tester.longPress(find.text('my words'));
    await settle(tester);
    expect(find.text(reportLabel), findsNothing);
    expect(find.text(blockLabel), findsNothing);
  });

  // spec: app-ui/the-admin-sees-reports
  testWidgets('the admin sees a report distinctly with a path to remove', (
    tester,
  ) async {
    final core = group()
      ..admin = true
      ..dmChats = [dmWithBob()];
    await openHome(tester, core);
    await tap(tester, find.byKey(const ValueKey('chat-1')));

    const report =
        '[vyajena-report v1]\n'
        'reported: mallory@chat\n'
        'reason: harassment\n'
        'kind: user\n';
    core.receive(bob, report, chatId: 1);
    await settle(tester);

    // Shown as a report card, not the raw block.
    expect(find.byKey(const Key('report-card')), findsOneWidget);
    expect(find.text(report), findsNothing);
    expect(find.textContaining('mallory'), findsOneWidget);
    expect(find.textContaining('harassment'), findsOneWidget);

    // A path to the existing remove-user screen.
    await tap(tester, find.byKey(const Key('report-remove')));
    expect(find.byType(RemoveUserScreen), findsOneWidget);
  });

  // spec: app-ui/the-admin-sees-reports
  testWidgets('a non-admin reporter sees no remove path on their report', (
    tester,
  ) async {
    final core = group()..dmChats = [dmWithBob()];
    await openHome(tester, core);
    await tap(tester, find.byKey(const ValueKey('chat-1')));

    const report =
        '[vyajena-report v1]\n'
        'reported: mallory@chat\n'
        'reason: abuse\n'
        'kind: user\n';
    core.receive(bob, report, chatId: 1);
    await settle(tester);

    expect(find.byKey(const Key('report-card')), findsOneWidget);
    expect(find.byKey(const Key('report-remove')), findsNothing);
  });

  // spec: app-ui/the-admin-sees-reports
  testWidgets('the chat list shows a plain line for a report, not the block', (
    tester,
  ) async {
    final core = group()
      ..admin = true
      ..dmChats = [dmWithBob()];
    const report =
        '[vyajena-report v1]\n'
        'reported: mallory@chat\n'
        'reason: harassment\n'
        'kind: user\n';
    core.receive(bob, report, chatId: 1);
    await openHome(tester, core);

    final preview = find.byKey(const ValueKey('chat-preview-1'));
    expect(preview, findsOneWidget);
    expect(tester.widget<Text>(preview).data, '⚠️ Report received');
    expect(find.textContaining('vyajena-report'), findsNothing);
  });
}
