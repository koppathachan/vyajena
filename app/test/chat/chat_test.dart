// app-ui: Chat screen.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/chat/message_tile.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

Future<FakeCoreService> aliceInChat(WidgetTester tester) async {
  final core = FakeCoreService(storedUser: alice)
    ..displayNames = {'alice@pi': 'Ali (alice)', 'bob@pi': 'Bob (bob)'};
  return openApp(tester, core);
}

Future<int> send(WidgetTester tester, FakeCoreService core, String text) async {
  await tester.enterText(find.byKey(const Key('composer')), text);
  await tap(tester, find.byTooltip('Send'));
  return core.messages.last.id;
}

/// The message tile showing [text].
Finder tile(String text) =>
    find.ancestor(of: find.text(text), matching: find.byType(MessageTile));

/// Whether [first] is drawn above [second].
bool above(WidgetTester tester, String first, String second) =>
    tester.getTopLeft(find.text(first)).dy <
    tester.getTopLeft(find.text(second)).dy;

void main() {
  // spec: app-ui/chat-screen
  testWidgets('Alice and Bob chat: "hi" then "hey", and "hi" goes from ⏳ to '
      '✓', (tester) async {
    final core = await aliceInChat(tester);
    final id = await send(tester, core, 'hi');
    expect(core.sent, ['hi']);
    expect(find.text('hi'), findsOneWidget);
    expect(
      find.descendant(of: tile('hi'), matching: find.text('⏳')),
      findsOneWidget,
    );

    core.setStatus(id, const SendState.accepted());
    await settle(tester);
    expect(
      find.descendant(of: tile('hi'), matching: find.text('✓')),
      findsOneWidget,
    );
    expect(find.text('⏳'), findsNothing);

    core.receive(bob, 'hey');
    await settle(tester);
    expect(above(tester, 'hi', 'hey'), isTrue);
    // Each sender by display name and username.
    expect(
      find.descendant(of: tile('hey'), matching: find.text('Bob (bob)')),
      findsOneWidget,
    );
    // Own messages carry no sender label.
    expect(
      find.descendant(of: tile('hi'), matching: find.text('Ali (alice)')),
      findsNothing,
    );
    // The composer was cleared after sending.
    expect(
      tester
          .widget<TextField>(find.byKey(const Key('composer')))
          .controller
          ?.text,
      '',
    );
  });

  // spec: app-ui/chat-screen
  testWidgets('Own messages look like own: right-aligned, no wider than '
      'the text, no label, ✓ at the end of the bubble', (tester) async {
    final core = FakeCoreService(storedUser: bob)
      ..displayNames = {'bob@pi': 'Bob (bob)', 'alice@pi': 'Ali (alice)'};
    await openApp(tester, core);
    final id = await send(tester, core, 'hi');
    core.setStatus(id, const SendState.accepted());
    core.receive(alice, 'hello there');
    await settle(tester);

    final screen = tester.getSize(find.byType(Scaffold).first);
    final mine = tester.getRect(
      find.descendant(of: tile('hi'), matching: find.byType(Card)),
    );
    final theirs = tester.getRect(
      find.descendant(of: tile('hello there'), matching: find.byType(Card)),
    );
    // Right-aligned and sized to "hi ✓", not stretched across the screen.
    expect(mine.right, closeTo(screen.width, 1));
    expect(mine.width, lessThan(screen.width / 3));
    expect(theirs.left, closeTo(0, 1));
    // No "bob" label; the ✓ sits in the bubble, on the line of the text.
    expect(
      find.descendant(of: tile('hi'), matching: find.text('Bob (bob)')),
      findsNothing,
    );
    final check = find.descendant(of: tile('hi'), matching: find.text('✓'));
    expect(check, findsOneWidget);
    expect(
      tester.getCenter(check).dy,
      closeTo(tester.getCenter(find.text('hi')).dy, 4),
    );
    expect(
      tester.getTopLeft(check).dx,
      greaterThan(tester.getTopRight(find.text('hi')).dx),
    );
    // Other users' messages keep their sender.
    expect(
      find.descendant(
        of: tile('hello there'),
        matching: find.text('Ali (alice)'),
      ),
      findsOneWidget,
    );
  });

  // spec: app-ui/chat-screen
  testWidgets('a long own message wraps inside the bubble and keeps its '
      'status at the end', (tester) async {
    final core = FakeCoreService(storedUser: bob);
    await openApp(tester, core);
    final long = List.filled(40, 'word').join(' ');
    await send(tester, core, long);
    final screen = tester.getSize(find.byType(Scaffold).first);
    final bubble = tester.getRect(
      find.descendant(of: tile(long), matching: find.byType(Card)),
    );
    expect(bubble.right, closeTo(screen.width, 1));
    expect(bubble.width, lessThanOrEqualTo(480));
    final mark = find.descendant(of: tile(long), matching: find.text('⏳'));
    expect(mark, findsOneWidget);
    expect(tester.getBottomRight(mark).dy, lessThanOrEqualTo(bubble.bottom));
  });

  // spec: app-ui/chat-screen
  testWidgets('Message from the user\'s other device: right-aligned, no '
      'label and no send status', (tester) async {
    final core = FakeCoreService(storedUser: bob)
      ..displayNames = {'bob@pi': 'Bobby (bob)'}
      ..messages = [
        const ChatMessage(
          id: 1,
          chatId: 0,
          sender: bob,
          senderDevice: 'ef56',
          outgoing: false,
          ownUser: true,
          body: ChatBody.text(text: 'from the tablet'),
          mentions: [],
          mentionsMe: false,
          acceptedAtMs: 1,
          status: SendState.accepted(),
        ),
      ];
    await openApp(tester, core);
    final screen = tester.getSize(find.byType(Scaffold).first);
    final bubble = tester.getRect(
      find.descendant(of: tile('from the tablet'), matching: find.byType(Card)),
    );
    expect(bubble.right, closeTo(screen.width, 1));
    expect(
      find.descendant(
        of: tile('from the tablet'),
        matching: find.text('Bobby (bob)'),
      ),
      findsNothing,
    );
    expect(find.text('✓'), findsNothing);
    expect(find.text('⏳'), findsNothing);
  });

  // spec: app-ui/chat-screen
  testWidgets('Message from the user\'s other device, on another member\'s '
      'phone: on the left as "Bobby (bob)"', (tester) async {
    final core = FakeCoreService(storedUser: alice)
      ..displayNames = {'bob@pi': 'Bobby (bob)'};
    await openApp(tester, core);
    core.receive(bob, 'from the tablet');
    await settle(tester);
    final bubble = tester.getRect(
      find.descendant(of: tile('from the tablet'), matching: find.byType(Card)),
    );
    expect(bubble.left, closeTo(0, 1));
    expect(
      find.descendant(
        of: tile('from the tablet'),
        matching: find.text('Bobby (bob)'),
      ),
      findsOneWidget,
    );
  });

  // spec: app-ui/chat-screen
  testWidgets('Sent across a commit: one message going from ⏳ to ✓, never '
      'two', (tester) async {
    final core = await aliceInChat(tester);
    final id = await send(tester, core, 'hi');
    // STALE_EPOCH: the core resends it in the new epoch; it stays sending.
    core.setStatus(id, const SendState.sending());
    await settle(tester);
    expect(find.text('hi'), findsOneWidget);
    expect(find.text('⏳'), findsOneWidget);
    core.setStatus(id, const SendState.accepted());
    await settle(tester);
    expect(find.text('hi'), findsOneWidget);
    expect(find.text('✓'), findsOneWidget);
  });

  // spec: app-ui/chat-screen
  testWidgets('a rejected message shows that it failed, with the reason', (
    tester,
  ) async {
    final core = await aliceInChat(tester);
    final id = await send(tester, core, 'hi');
    core.setStatus(id, const SendState.failed(reason: 'too large for the Pi'));
    await settle(tester);
    expect(find.text('Failed: too large for the Pi'), findsOneWidget);
  });

  // spec: app-ui/chat-screen
  testWidgets('messages follow the history\'s server_seq order, also when '
      'an own message is accepted after a received one', (tester) async {
    final core = await aliceInChat(tester);
    final id = await send(tester, core, 'mine');
    core.receive(bob, 'theirs');
    await settle(tester);
    // The Pi sequenced "mine" first; the core's history says so once it is
    // accepted.
    final mine = core.messages.firstWhere((m) => m.id == id);
    core.messages
      ..remove(mine)
      ..insert(0, mine);
    core.setStatus(id, const SendState.accepted());
    await settle(tester);
    expect(above(tester, 'mine', 'theirs'), isTrue);

    // And the other way round.
    final theirs = core.messages.firstWhere((m) => m.id != id);
    core.messages
      ..remove(theirs)
      ..insert(0, theirs);
    core.emit(CoreEvent.message(message: theirs));
    await settle(tester);
    expect(above(tester, 'theirs', 'mine'), isTrue);
  });

  // spec: app-ui/chat-screen
  testWidgets('the history is shown when the chat opens', (tester) async {
    final core = FakeCoreService(storedUser: alice)
      ..messages = [
        const ChatMessage(
          id: 1,
          chatId: 0,
          sender: bob,
          senderDevice: 'cd34',
          outgoing: false,
          ownUser: false,
          body: ChatBody.text(text: 'earlier'),
          mentions: [],
          mentionsMe: false,
          acceptedAtMs: 1,
        ),
      ];
    await openApp(tester, core);
    expect(find.text('earlier'), findsOneWidget);
  });

  // spec: app-ui/chat-screen
  testWidgets('Unknown body: a placeholder saying the app needs an update', (
    tester,
  ) async {
    final core = await aliceInChat(tester);
    final message = ChatMessage(
      id: 99,
      chatId: 0,
      sender: bob,
      senderDevice: 'cd34',
      outgoing: false,
      ownUser: false,
      body: const ChatBody.unsupported(),
      mentions: const [],
      mentionsMe: false,
      acceptedAtMs: 1,
    );
    core.messages.add(message);
    core.emit(CoreEvent.message(message: message));
    await settle(tester);
    expect(find.text(unsupportedText), findsOneWidget);
  });

  // spec: app-ui/chat-screen
  testWidgets('an Unreadable event shows that a message could not be read', (
    tester,
  ) async {
    final core = await aliceInChat(tester);
    core.emit(
      const CoreEvent.unreadable(sender: bob, acceptedAtMs: 5, chatId: 0),
    );
    await settle(tester);
    expect(
      find.text('A message from Bob (bob) could not be read.'),
      findsOneWidget,
    );
  });
}
