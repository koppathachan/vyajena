// The app on a simulator or emulator against a real temp server (flutter-app
// task 6.2, design D6/D7). Run it with `integration_test/run.sh <device>`:
// the script starts the server in a temp folder and the admin's device on the
// host (`bridge/tests/app_peer.rs`, driven through the core), and passes the
// invite for `bob` in as VYAJENA_INVITE. This app is Bob's phone.
//
// End to end: Bob joins from the invite, the admin's device adds him on its
// own (no one taps anything), Bob's "hi" goes from ⏳ to ✓, and the admin's
// "hey" arrives.
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:vyajena/app/app.dart';
import 'package:vyajena/core/rust_core_service.dart';
import 'package:vyajena/src/rust/frb_generated.dart';

/// The invite text the admin's device made for `bob`.
const invite = String.fromEnvironment('VYAJENA_INVITE');

/// Pumps until [finder] finds something, in real time.
Future<void> waitFor(
  WidgetTester tester,
  Finder finder, {
  Duration timeout = const Duration(seconds: 90),
}) async {
  final end = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(end)) {
    await tester.pump(const Duration(milliseconds: 50));
    if (finder.evaluate().isNotEmpty) return;
    await Future<void>.delayed(const Duration(milliseconds: 100));
  }
  fail('timed out waiting for $finder');
}

Future<void> tap(WidgetTester tester, Finder finder) async {
  await waitFor(tester, finder);
  await tester.tap(finder);
  await tester.pump(const Duration(milliseconds: 100));
}

/// The status mark of the own message [text] ("⏳" or "✓").
Finder statusOf(String text, String mark) => find.descendant(
  of: find.ancestor(of: find.text(text), matching: find.byType(Card)),
  matching: find.text(mark),
);

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  // spec: app-ui/join-from-an-invite
  // spec: app-ui/chat-screen
  // spec: group-membership/automatic-add-of-new-users
  testWidgets('Bob joins from the invite, is added by the admin\'s device '
      'on its own, and chats: "hi" goes from ⏳ to ✓ and "hey" arrives', (
    tester,
  ) async {
    expect(invite, isNotEmpty, reason: 'run integration_test/run.sh');

    // A fresh phone: no data from an earlier run (keys in the key store are
    // reused by the core's get-or-create, which is fine for a new server).
    final dataDir = await platformChannel.invokeMethod<String>('dataDir');
    for (final entry in Directory(dataDir!).listSync()) {
      entry.deleteSync(recursive: true);
    }

    await RustLib.init();
    final core = RustCoreService();
    await tester.pumpWidget(VyajenaApp(core: core));
    await core.open();

    // Join: the invite names bob@chat on the temp server before anything is sent.
    await waitFor(tester, find.text('Join the group'));
    await tester.enterText(find.byKey(const Key('invite')), invite);
    await tap(tester, find.text('Check invite'));
    await waitFor(tester, find.text('bob@chat'));
    expect(find.textContaining('on 127.0.0.1:'), findsOneWidget);
    await tap(tester, find.text('Join'));

    // Waiting, then the admin's device adds this phone by itself: the chat.
    await waitFor(tester, find.byKey(const Key('composer')));
    expect(find.text('No messages yet'), findsOneWidget);

    // Offline, "hi" stays ⏳; once the core runs again the server accepts it.
    await core.stop();
    await tester.enterText(find.byKey(const Key('composer')), 'hi');
    await tap(tester, find.byTooltip('Send'));
    await waitFor(tester, statusOf('hi', '⏳'));
    await core.start();
    await waitFor(tester, statusOf('hi', '✓'));
    expect(statusOf('hi', '⏳'), findsNothing);

    // The admin's device got "hi" and replied.
    await waitFor(tester, find.text('hey'));
    expect(
      tester.getTopLeft(find.text('hi')).dy <
          tester.getTopLeft(find.text('hey')).dy,
      isTrue,
    );
    expect(find.text('akhil'), findsOneWidget);
    expect(find.byTooltip('Connected'), findsOneWidget);

    await core.stop();
  });
}
