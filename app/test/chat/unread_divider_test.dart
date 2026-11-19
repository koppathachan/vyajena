// app-ui: Unread divider; Scroll to bottom.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

/// A received message from Bob (or Alice's own device when [own]).
ChatMessage msg(int id, String text, {bool own = false}) => ChatMessage(
  id: id,
  chatId: 0,
  sender: own ? alice : bob,
  senderDevice: own ? 'ab12' : 'cd34',
  outgoing: own,
  ownUser: own,
  body: ChatBody.text(text: text),
  mentions: const [],
  mentionsMe: false,
  acceptedAtMs: 1,
  status: own ? const SendState.accepted() : null,
);

double dyOf(WidgetTester tester, Finder finder) => tester.getTopLeft(finder).dy;

Finder divider() => find.byKey(const Key('unread-divider'));
Finder fab() => find.byKey(const Key('scroll-to-bottom'));

void main() {
  // spec: app-ui/unread-divider
  testWidgets('a "New messages" divider sits below the last read and above '
      'the first unread, and the chat opens on it', (tester) async {
    final core = FakeCoreService(storedUser: alice)
      ..messages = [msg(1, 'one'), msg(2, 'two'), msg(3, 'three')]
      ..readMarks[0] = 1;
    await openApp(tester, core);

    expect(divider(), findsOneWidget);
    // one (read) is above the divider; two and three (unread) are below it.
    expect(dyOf(tester, find.text('one')), lessThan(dyOf(tester, divider())));
    expect(dyOf(tester, divider()), lessThan(dyOf(tester, find.text('two'))));
    expect(find.text('two'), findsOneWidget);
  });

  // spec: app-ui/unread-divider
  testWidgets('no divider when everything has been read', (tester) async {
    final core = FakeCoreService(storedUser: alice)
      ..messages = [msg(1, 'one'), msg(2, 'two')]
      ..readMarks[0] = 2;
    await openApp(tester, core);
    expect(divider(), findsNothing);
  });

  // spec: app-ui/unread-divider
  testWidgets('no divider when the chat has never been read (all unread)', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: alice)
      ..messages = [msg(1, 'one'), msg(2, 'two'), msg(3, 'three')]
      ..readMarks[0] = 0;
    await openApp(tester, core);
    // The first unread is the oldest message shown: no divider above it.
    expect(divider(), findsNothing);
  });

  // spec: app-ui/unread-divider
  testWidgets('an own message past the mark does not pull the divider', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: alice)
      ..messages = [msg(1, 'one'), msg(2, 'mine', own: true), msg(3, 'theirs')]
      ..readMarks[0] = 1;
    await openApp(tester, core);
    // The divider marks "theirs" (received), not "mine" (own).
    expect(divider(), findsOneWidget);
    expect(dyOf(tester, find.text('mine')), lessThan(dyOf(tester, divider())));
    expect(
      dyOf(tester, divider()),
      lessThan(dyOf(tester, find.text('theirs'))),
    );
  });

  // spec: app-ui/unread-divider
  testWidgets('the divider stays put while a new message arrives', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: alice)
      ..messages = [msg(1, 'one'), msg(2, 'two')]
      ..readMarks[0] = 1;
    await openApp(tester, core);
    expect(divider(), findsOneWidget);

    core.receive(bob, 'four');
    await settle(tester);

    // Still between "one" and "two", not moved down to "four".
    expect(dyOf(tester, find.text('one')), lessThan(dyOf(tester, divider())));
    expect(dyOf(tester, divider()), lessThan(dyOf(tester, find.text('two'))));
    expect(
      dyOf(tester, find.text('two')),
      lessThan(dyOf(tester, find.text('four'))),
    );
  });

  // spec: app-ui/scroll-to-bottom
  testWidgets('the scroll-to-bottom button shows only when scrolled up and '
      'returns to the newest message', (tester) async {
    final core = FakeCoreService(storedUser: alice)
      ..messages = [for (var i = 1; i <= 40; i++) msg(i, 'line $i')]
      ..readMarks[0] = 40;
    await openApp(tester, core);

    // At the bottom (newest): no button.
    expect(fab(), findsNothing);

    // Scroll up through the history.
    await tester.drag(find.byType(ListView), const Offset(0, 400));
    await settle(tester);
    expect(fab(), findsOneWidget);

    // Tapping it returns to the bottom and hides the button.
    await tester.tap(fab());
    await settle(tester);
    expect(fab(), findsNothing);
  });
}
