// app-ui: Compact message runs.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/chat/message_tile.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

ChatMessage msg(
  int id,
  String text, {
  bool own = false,
  int at = 1,
  UserRef? from,
}) => ChatMessage(
  id: id,
  chatId: 0,
  sender: from ?? (own ? alice : bob),
  senderDevice: own ? 'ab12' : 'cd34',
  outgoing: own,
  ownUser: own,
  body: ChatBody.text(text: text),
  mentions: const [],
  mentionsMe: false,
  acceptedAtMs: at,
  status: own ? const SendState.accepted() : null,
);

Finder time(int id) => find.byKey(ValueKey('time-$id'));

void main() {
  // spec: app-ui/compact-message-runs
  test('one sender within two minutes is one run; another sender or a longer '
      'gap is not', () {
    expect(sameRun(msg(1, 'a'), msg(2, 'b', at: 60000)), isTrue);
    expect(sameRun(msg(1, 'a'), msg(2, 'b', at: runGapMs + 2)), isFalse);
    expect(sameRun(msg(1, 'a'), msg(2, 'b', own: true)), isFalse);
  });

  // spec: app-ui/compact-message-runs
  testWidgets('a run shows its time on the last bubble only; a gap starts a '
      'new run', (tester) async {
    final core = FakeCoreService(storedUser: alice)
      ..messages = [
        msg(1, 'one', at: 1000),
        msg(2, 'two', at: 2000),
        msg(3, 'three', at: 3000),
        msg(4, 'later', at: 3000 + runGapMs + 1000),
      ];
    await openApp(tester, core);

    expect(time(1), findsNothing);
    expect(time(2), findsNothing);
    expect(time(3), findsOneWidget);
    expect(time(4), findsOneWidget);
  });
}
