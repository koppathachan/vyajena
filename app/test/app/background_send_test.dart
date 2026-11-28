// app-ui: Finish sends in the background (change finish-sends-in-background).
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/app/app.dart';
import 'package:vyajena/app/lifecycle.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/fake_background_task.dart';
import '../support/fake_core_service.dart';

const bob = UserRef(username: 'bob', serverId: 'chat');

Future<(FakeCoreService, FakeBackgroundTask)> openApp(
  WidgetTester tester, {
  required bool unsent,
}) async {
  final core = FakeCoreService(storedUser: bob)..unsent = unsent;
  final background = FakeBackgroundTask();
  await tester.pumpWidget(VyajenaApp(core: core, background: background));
  await core.open();
  await tester.pump();
  return (core, background);
}

void pause(WidgetTester tester) {
  final b = tester.binding;
  b.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
  b.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
  b.handleAppLifecycleStateChanged(AppLifecycleState.paused);
}

void resume(WidgetTester tester) {
  final b = tester.binding;
  b.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
  b.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
  b.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
}

void connected(FakeCoreService core) =>
    core.emit(const CoreEvent.connection(status: ConnectionStatus.connected));

int stops(FakeCoreService core) => core.calls.where((c) => c == 'stop').length;

void main() {
  // spec: app-ui/finish-sends-in-the-background
  testWidgets('Nothing to send: the core stops at once, as before', (
    tester,
  ) async {
    final (core, background) = await openApp(tester, unsent: false);
    connected(core);
    pause(tester);
    await tester.pump();
    expect(stops(core), 1);
    expect(background.calls, ['begin', 'end']);
  });

  // spec: app-ui/finish-sends-in-the-background
  testWidgets('Message sends in the background: the core stops once it is '
      'accepted', (tester) async {
    final (core, background) = await openApp(tester, unsent: true);
    connected(core);
    await tester.pump();
    pause(tester);
    await tester.pump(const Duration(seconds: 5));
    expect(stops(core), 0, reason: 'a message is still sending');
    expect(background.calls, ['begin']);

    core.unsent = false;
    core.emit(const CoreEvent.sendStatus(id: 1, status: SendState.accepted()));
    await tester.pump();
    expect(stops(core), 1);
    expect(background.calls, ['begin', 'end']);
  });

  // spec: app-ui/finish-sends-in-the-background
  testWidgets('One minute after connecting: the core stops, counted from the '
      'connection', (tester) async {
    final (core, background) = await openApp(tester, unsent: true);
    pause(tester);
    await tester.pump(const Duration(seconds: 20));
    connected(core);
    await tester.pump();
    // 59 s after the connection (79 s after leaving): still running.
    await tester.pump(kBackgroundSendLimit - const Duration(seconds: 1));
    expect(stops(core), 0);
    await tester.pump(const Duration(seconds: 2));
    expect(stops(core), 1);
    expect(background.calls, ['begin', 'end']);
  });

  // spec: app-ui/finish-sends-in-the-background
  testWidgets('No connection: the core stops after a minute', (tester) async {
    final (core, _) = await openApp(tester, unsent: true);
    pause(tester);
    await tester.pump(kBackgroundConnectWait - const Duration(seconds: 1));
    expect(stops(core), 0);
    await tester.pump(const Duration(seconds: 2));
    expect(stops(core), 1);
  });

  // spec: app-ui/finish-sends-in-the-background
  testWidgets('Back to the app: the wait ends and the core keeps running', (
    tester,
  ) async {
    final (core, background) = await openApp(tester, unsent: true);
    connected(core);
    await tester.pump();
    pause(tester);
    await tester.pump(const Duration(seconds: 10));
    resume(tester);
    await tester.pump(const Duration(minutes: 3));
    expect(stops(core), 0);
    expect(
      core.calls.where((c) => c == 'start').length,
      1,
      reason: 'no restart',
    );
    expect(background.calls, ['begin', 'end']);
  });

  // spec: app-ui/finish-sends-in-the-background
  testWidgets('The system ends the background time: the core stops at once', (
    tester,
  ) async {
    final (core, background) = await openApp(tester, unsent: true);
    connected(core);
    await tester.pump();
    pause(tester);
    await tester.pump(const Duration(seconds: 5));
    background.expire();
    await tester.pump();
    expect(stops(core), 1);
    expect(background.calls, ['begin', 'end']);
  });

  // spec: app-ui/stop-the-core-when-the-phone-locks
  testWidgets('Lock during a background send: the core stops at once', (
    tester,
  ) async {
    final (core, background) = await openApp(tester, unsent: true);
    connected(core);
    await tester.pump();
    pause(tester);
    await tester.pump(const Duration(seconds: 5));
    expect(stops(core), 0, reason: 'still finishing the send');
    expect(background.calls, ['begin']);
    // The phone locks: the core must stop before its background tick reads the
    // now-unreadable NSFileProtectionComplete database (would SIGBUS).
    background.lock();
    await tester.pump();
    expect(stops(core), 1);
    expect(background.calls, ['begin', 'end']);
  });

  // spec: app-ui/stop-the-core-when-the-phone-locks
  testWidgets('Already locked when backgrounded: the core stops at once, no '
      'wait', (tester) async {
    final (core, background) = await openApp(tester, unsent: true);
    background.available = false; // the phone is already locked
    connected(core);
    await tester.pump();
    pause(tester);
    await tester.pump();
    await tester.pump();
    expect(stops(core), 1, reason: 'no wait while protected data is unavailable');
    expect(background.calls, ['begin', 'end']);
  });
}
