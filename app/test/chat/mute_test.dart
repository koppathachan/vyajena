// app-ui: Mute a conversation from the chat list; Mute the whole app
// (change `mute-notifications`).
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/app.dart';
import 'dms_test.dart' show aliceInGroup;

/// A tall screen so the whole Profile tab is built and on screen.
void tall(WidgetTester tester) {
  tester.view.physicalSize = const Size(800, 2400);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
}

void main() {
  // spec: app-ui/mute-a-conversation-from-the-chat-list
  testWidgets('long-pressing the lobby row and tapping Mute records the mute '
      'and shows the marker', (tester) async {
    final core = aliceInGroup();
    await openHome(tester, core);

    final lobby = find.byKey(const ValueKey('chat-main'));
    expect(lobby, findsOneWidget);
    expect(find.byKey(const ValueKey('chat-muted-0')), findsNothing);

    await tester.longPress(lobby);
    await settle(tester);
    await tap(tester, find.byKey(const ValueKey('chat-mute-toggle-0')));

    expect(core.calls, contains('setConversationMuted'));
    expect(core.args['setConversationMuted'], [0, true]);
    expect(find.byKey(const ValueKey('chat-muted-0')), findsOneWidget);
  });

  // spec: app-ui/mute-a-conversation-from-the-chat-list
  testWidgets('a muted row is unmuted from its long-press menu', (tester) async {
    final core = aliceInGroup()..mutedChats.add(0);
    await openHome(tester, core);
    expect(find.byKey(const ValueKey('chat-muted-0')), findsOneWidget);

    await tester.longPress(find.byKey(const ValueKey('chat-main')));
    await settle(tester);
    // The menu offers Unmute for an already-muted conversation.
    await tap(tester, find.text('Unmute notifications'));

    expect(core.args['setConversationMuted'], [0, false]);
    expect(find.byKey(const ValueKey('chat-muted-0')), findsNothing);
  });

  // spec: app-ui/mute-the-whole-app
  testWidgets('the Profile tab mutes all notifications and the chat list shows '
      'the global banner', (tester) async {
    tall(tester);
    final core = aliceInGroup();
    await openHome(tester, core);
    expect(find.byKey(const Key('app-muted-banner')), findsNothing);

    // Turn on "mute all" in the Profile tab.
    await tap(tester, find.byKey(const Key('tab-profile')));
    final toggle = find.byKey(const Key('mute-all-notifications'));
    expect(toggle, findsOneWidget);
    await tap(tester, toggle);
    expect(core.calls, contains('setAppMuted'));
    expect(core.args['setAppMuted'], true);

    // Back on Chats the banner conveys that notifications are globally off.
    await tap(tester, find.byKey(const Key('tab-chats')));
    expect(find.byKey(const Key('app-muted-banner')), findsOneWidget);
  });

  // spec: app-ui/mute-the-whole-app
  testWidgets('with mute-all on, the banner shows and per-row markers do not', (
    tester,
  ) async {
    final core = aliceInGroup()
      ..mutedChats.add(0)
      ..appMutedValue = true;
    await openHome(tester, core);

    expect(find.byKey(const Key('app-muted-banner')), findsOneWidget);
    // The lobby was muted individually, but mute-all is shown globally, not
    // as a per-row marker.
    expect(find.byKey(const ValueKey('chat-muted-0')), findsNothing);
  });
}
