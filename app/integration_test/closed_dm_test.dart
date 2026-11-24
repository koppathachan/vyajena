// Removing a closed DM on a simulator or emulator against a real temp server
// (change `remove-closed-dm`). Run it with `integration_test/run.sh <device>
// closeddm`: the script starts the server and, on the host, the admin's device
// and Carol's (bridge/tests/app_peer_closed_dm.rs). This app is Bob's phone.
//
// End to end: Carol DMs Bob "bye"; Bob answers "ready" in the lobby; the admin
// removes Carol, so the DM closes; Bob swipes it, is asked to confirm (Cancel
// keeps it), confirms, and the row and its messages are gone. The script then
// checks the server log for the Dms.RemoveClosedDm call.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:vyajena/src/rust/api/types.dart';

import 'support.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  // spec: app-ui/remove-a-closed-dm-from-the-list
  // spec: message-deletion/remove-a-closed-dm
  testWidgets('a closed DM is removed after a confirm, with its messages', (
    tester,
  ) async {
    final core = await joinFresh(tester);

    // Carol's DM and her "bye" reach this phone.
    late int dm;
    final end = DateTime.now().add(const Duration(seconds: 90));
    while (true) {
      await tester.pump(const Duration(milliseconds: 100));
      final chats = await core.chats();
      final found = chats.where((c) => c.kind is ChatKindInfo_Dm);
      if (found.isNotEmpty) {
        final id = found.first.chatId;
        final history = await core.historyIn(id, 10);
        if (history.any(
          (m) =>
              m.body is ChatBody_Text &&
              (m.body as ChatBody_Text).text == 'bye',
        )) {
          dm = id;
          break;
        }
      }
      if (DateTime.now().isAfter(end)) fail('no DM with "bye"');
      await Future<void>.delayed(const Duration(milliseconds: 200));
    }

    // Tell the host peer, which then removes Carol and closes the DM.
    await tester.enterText(find.byKey(const Key('composer')), 'ready');
    await tap(tester, find.byTooltip('Send'));
    await waitFor(tester, statusOf('ready', '✓'));
    await tester.pageBack();
    await tester.pump(const Duration(milliseconds: 500));

    // The row turns closed: only then does it have a swipe.
    final row = find.byKey(ValueKey('dismiss-$dm'));
    await waitFor(tester, row);
    await tester.drag(row, const Offset(-600, 0));
    await tester.pump(const Duration(milliseconds: 500));
    await waitFor(tester, find.byKey(const Key('remove-closed-dm-dialog')));

    // Cancel keeps the chat and its messages.
    await tap(tester, find.byKey(const Key('remove-closed-dm-cancel')));
    await tester.pump(const Duration(milliseconds: 500));
    expect(row, findsOneWidget);
    expect((await core.historyIn(dm, 10)), isNotEmpty);

    // Confirm removes it.
    await tester.drag(row, const Offset(-600, 0));
    await tester.pump(const Duration(milliseconds: 500));
    await tap(tester, find.byKey(const Key('remove-closed-dm-confirm')));
    await waitForCount(tester, find.byKey(ValueKey('chat-$dm')), 0);
    final chats = await core.chats();
    expect(chats.where((c) => c.kind is ChatKindInfo_Dm), isEmpty);
    // Its messages are gone from this phone.
    var gone = false;
    try {
      gone = (await core.historyIn(dm, 10)).isEmpty;
    } catch (_) {
      gone = true;
    }
    expect(gone, isTrue);

    await core.stop();
  });
}
