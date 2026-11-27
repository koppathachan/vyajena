// app-ui: Copy a message; Forward a message (change `message-actions-share`).
// The long-press sheet's Copy (text or caption) and Forward (text), and the
// forward picker that excludes the current and closed chats.
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
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

ChatSummary dm(int id, UserRef peer, {bool closed = false}) => ChatSummary(
  chatId: id,
  kind: ChatKindInfo.dm(peer: peer, waiting: false, closed: closed),
  unread: 0,
  unseenMentions: 0,
  muted: false,
);

/// Captures what the app copies to the clipboard, readable back through
/// `Clipboard.getData`.
void mockClipboard(WidgetTester tester) {
  String? copied;
  tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
    SystemChannels.platform,
    (call) async {
      if (call.method == 'Clipboard.setData') {
        copied = (call.arguments as Map)['text'] as String?;
      }
      if (call.method == 'Clipboard.getData') {
        return <String, dynamic>{'text': copied};
      }
      return null;
    },
  );
  addTearDown(
    () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      null,
    ),
  );
}

void main() {
  // spec: app-ui/copy-a-message
  testWidgets('Copy puts a text message on the clipboard', (tester) async {
    final core = group();
    await openApp(tester, core);
    mockClipboard(tester);
    core.receive(bob, 'meet at 6');
    await settle(tester);

    await tester.longPress(find.text('meet at 6'));
    await settle(tester);
    expect(find.text(copyLabel), findsOneWidget);
    await tap(tester, find.text(copyLabel));

    final data = await Clipboard.getData(Clipboard.kTextPlain);
    expect(data?.text, 'meet at 6');
    expect(find.text('Copied'), findsOneWidget);
  });

  // spec: app-ui/copy-a-message
  testWidgets('Copy puts an image caption on the clipboard', (tester) async {
    final core = group();
    await openApp(tester, core);
    mockClipboard(tester);
    core.receiveImage(bob, caption: 'on the roof');
    await settle(tester);

    await tester.longPress(find.byKey(const Key('image-bubble')));
    await settle(tester);
    expect(find.text(copyLabel), findsOneWidget);
    await tap(tester, find.text(copyLabel));

    final data = await Clipboard.getData(Clipboard.kTextPlain);
    expect(data?.text, 'on the roof');
  });

  // spec: app-ui/copy-a-message
  testWidgets('a media-only message offers neither Copy nor Forward', (
    tester,
  ) async {
    final core = group();
    await openApp(tester, core);
    core.receiveImage(bob);
    await settle(tester);

    await tester.longPress(find.byKey(const Key('image-bubble')));
    await settle(tester);
    expect(find.text(replyLabel), findsOneWidget);
    expect(find.text(copyLabel), findsNothing);
    expect(find.text(forwardLabel), findsNothing);
  });

  // spec: app-ui/forward-a-message
  testWidgets('Forward sends text into the chosen chat', (tester) async {
    final core = group()..dmChats = [dm(1, bob)];
    await openHome(tester, core);
    await enterGroup(tester);
    core.receive(bob, 'hello');
    await settle(tester);

    await tester.longPress(find.text('hello'));
    await settle(tester);
    expect(find.text(forwardLabel), findsOneWidget);
    await tap(tester, find.text(forwardLabel));

    // The picker lists the DM (chat 1) and excludes the current lobby (chat 0).
    expect(find.byKey(const Key('forward-picker')), findsOneWidget);
    expect(find.byKey(const Key('forward-to-1')), findsOneWidget);
    expect(find.byKey(const Key('forward-to-0')), findsNothing);

    await tap(tester, find.byKey(const Key('forward-to-1')));
    expect(core.calls, contains('sendMessageIn'));
    final sent =
        core.args['sendMessageIn']
            as ({int chatId, String text, List<MentionSpan> mentions});
    expect(sent.chatId, 1);
    expect(sent.text, 'hello');
    expect(core.sent, contains('hello'));
  });

  // spec: app-ui/forward-a-message
  testWidgets('the forward picker hides a closed DM', (tester) async {
    final core = group()..dmChats = [dm(1, bob), dm(2, akhil, closed: true)];
    await openHome(tester, core);
    await enterGroup(tester);
    core.receive(bob, 'hi');
    await settle(tester);

    await tester.longPress(find.text('hi'));
    await settle(tester);
    await tap(tester, find.text(forwardLabel));

    expect(find.byKey(const Key('forward-to-1')), findsOneWidget);
    expect(find.byKey(const Key('forward-to-2')), findsNothing);
  });
}
