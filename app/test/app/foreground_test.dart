// app-ui: Foreground and background (flutter-app design D9).
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/app/app.dart';
import 'package:vyajena/app/connection_indicator.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/fake_core_service.dart';

const bob = UserRef(username: 'bob', serverId: 'chat');

Future<FakeCoreService> openApp(WidgetTester tester, {UserRef? user}) async {
  final core = FakeCoreService(storedUser: user);
  await tester.pumpWidget(VyajenaApp(core: core));
  await core.open();
  await tester.pump();
  return core;
}

void main() {
  // spec: app-ui/foreground-and-background
  testWidgets('the core starts when the app opens', (tester) async {
    final core = await openApp(tester, user: bob);
    expect(core.calls, ['open', 'start']);
    expect(core.state.value, CoreState.ready);
    expect(find.text('bob@chat'), findsOneWidget);
    expect(find.byTooltip('Connecting…'), findsOneWidget);

    core.emit(const CoreEvent.connection(status: ConnectionStatus.connected));
    await tester.pump();
    expect(find.byTooltip('Connected'), findsOneWidget);
    expect(
      tester.widget<Icon>(find.byIcon(Icons.cloud_done_outlined)).icon,
      Icons.cloud_done_outlined,
    );
  });

  // spec: app-ui/foreground-and-background
  testWidgets('a device without a user opens but does not start', (
    tester,
  ) async {
    final core = await openApp(tester);
    expect(core.calls, ['open']);
    expect(core.state.value, CoreState.unregistered);
    expect(find.byTooltip('Offline'), findsOneWidget);

    await core.register('vyajena-invite:bob');
    await tester.pump();
    expect(core.calls, ['open', 'register', 'start']);
    expect(find.text('bob@chat'), findsOneWidget);
  });

  // spec: app-ui/foreground-and-background
  testWidgets('Back to the app: the core reconnects and the indicator '
      'turns connected', (tester) async {
    final core = await openApp(tester, user: bob);
    core.emit(const CoreEvent.connection(status: ConnectionStatus.connected));
    await tester.pump();
    expect(find.byTooltip('Connected'), findsOneWidget);

    // Bob switches to another app for an hour.
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.paused);
    await tester.pump();
    expect(core.calls, ['open', 'start', 'stop']);
    // No frames are drawn in the background; the indicator's state is.
    expect(core.connection.value, ConnectionStatus.disconnected);

    // And back: the core restarts at once and catches up in order.
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await tester.pump();
    expect(core.calls, ['open', 'start', 'stop', 'start']);
    expect(find.byTooltip('Connecting…'), findsOneWidget);

    final seen = <CoreEvent>[];
    final subscription = core.events.listen(seen.add);
    addTearDown(subscription.cancel);
    final caughtUp = [
      for (var id = 1; id <= 3; id++)
        CoreEvent.sendStatus(id: id, status: const SendState.accepted()),
    ];
    for (final event in caughtUp) {
      core.emit(event);
    }
    core.emit(const CoreEvent.connection(status: ConnectionStatus.connected));
    await tester.pump();
    expect(seen.take(3), caughtUp);
    expect(find.byTooltip('Connected'), findsOneWidget);
  });

  // spec: app-ui/foreground-and-background
  testWidgets('a short interruption does not stop the core', (tester) async {
    final core = await openApp(tester, user: bob);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await tester.pump();
    expect(core.calls, ['open', 'start']);
  });

  // spec: app-ui/foreground-and-background
  testWidgets('the indicator shows each connection state', (tester) async {
    final core = await openApp(tester, user: bob);
    for (final status in ConnectionStatus.values) {
      core.emit(CoreEvent.connection(status: status));
      await tester.pump();
      expect(find.byTooltip(ConnectionIndicator.label(status)), findsOneWidget);
      expect(find.byIcon(ConnectionIndicator.icon(status)), findsOneWidget);
    }
  });

  // spec: app-ui/foreground-and-background
  testWidgets('a core that cannot open says so', (tester) async {
    final core = FakeCoreService(
      openFails: const CoreError(
        kind: CoreErrorKind.keyStore,
        message: 'no OS keychain is available',
      ),
    );
    await tester.pumpWidget(VyajenaApp(core: core));
    await core.open();
    await tester.pump();
    expect(core.state.value, CoreState.failed);
    expect(find.textContaining('no OS keychain is available'), findsOneWidget);
    expect(core.calls, ['open']);
  });

  // spec: app-ui/foreground-and-background
  testWidgets('Network back: the app calls reconnect_now when a connection '
      'comes up', (tester) async {
    final network = StreamController<bool>();
    addTearDown(network.close);
    final core = FakeCoreService(storedUser: bob);
    await tester.pumpWidget(
      VyajenaApp(core: core, networkChanges: network.stream),
    );
    await core.open();
    await tester.pump();
    core.emit(
      const CoreEvent.connection(status: ConnectionStatus.disconnected),
    );

    // Airplane mode on: nothing to reconnect over.
    network.add(false);
    await tester.pump();
    expect(core.calls, ['open', 'start']);

    // Airplane mode off: the core is told at once.
    network.add(true);
    await tester.pump();
    expect(core.calls, ['open', 'start', 'reconnectNow']);

    // Wi-Fi to mobile is a change too.
    network.add(true);
    await tester.pump();
    expect(core.calls, ['open', 'start', 'reconnectNow', 'reconnectNow']);
  });
}
