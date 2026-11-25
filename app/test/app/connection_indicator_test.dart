// app-ui: Foreground and background — the connection indicator looks
// different in each state (connecting never reads as offline) and retries on
// tap or pull (change `connect-feedback`).
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/app/connection_indicator.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

/// Renders just the indicator over [core], so a test drives the connection
/// state directly. Never `pumpAndSettle` here: connecting pulses forever.
Future<ColorScheme> pumpIndicator(
  WidgetTester tester,
  FakeCoreService core,
) async {
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        appBar: AppBar(actions: [ConnectionIndicator(core: core)]),
      ),
    ),
  );
  await tester.pump();
  return Theme.of(tester.element(find.byType(ConnectionIndicator))).colorScheme;
}

/// The one `FadeTransition` the indicator wraps its glyph in.
Finder fade() => find.descendant(
  of: find.byType(ConnectionIndicator),
  matching: find.byType(FadeTransition),
);

void main() {
  // spec: app-ui/foreground-and-background
  testWidgets('Connecting does not look offline: it pulses in the active '
      'colour, offline is static and muted', (tester) async {
    final core = FakeCoreService(storedUser: bob);

    // Connecting: the active colour, and the glyph's opacity moves.
    core.emit(const CoreEvent.connection(status: ConnectionStatus.connecting));
    final scheme = await pumpIndicator(tester, core);
    expect(find.byTooltip('Connecting…'), findsOneWidget);
    final connecting = tester.widget<Icon>(
      find.byIcon(ConnectionIndicator.icon(ConnectionStatus.connecting)),
    );
    expect(connecting.color, scheme.primary);

    final pulse = tester.widget<FadeTransition>(fade()).opacity;
    final before = pulse.value;
    expect(before, lessThan(1.0), reason: 'the pulse dims the glyph');
    await tester.pump(const Duration(milliseconds: 300));
    expect(pulse.value, isNot(before), reason: 'it keeps moving');

    // Offline: the muted colour, and the glyph is static at full opacity.
    core.emit(
      const CoreEvent.connection(status: ConnectionStatus.disconnected),
    );
    await tester.pump();
    expect(find.byTooltip('Offline'), findsOneWidget);
    final offline = tester.widget<Icon>(
      find.byIcon(ConnectionIndicator.icon(ConnectionStatus.disconnected)),
    );
    expect(offline.color, scheme.onSurfaceVariant);
    expect(offline.color, isNot(connecting.color), reason: 'distinct colours');

    final still = tester.widget<FadeTransition>(fade()).opacity;
    expect(still.value, 1.0);
    await tester.pump(const Duration(milliseconds: 300));
    expect(still.value, 1.0, reason: 'offline does not animate');
  });

  // spec: app-ui/foreground-and-background
  testWidgets('Connected is a static done glyph in the active colour', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: bob);
    core.emit(const CoreEvent.connection(status: ConnectionStatus.connected));
    final scheme = await pumpIndicator(tester, core);
    expect(find.byTooltip('Connected'), findsOneWidget);
    final icon = tester.widget<Icon>(
      find.byIcon(ConnectionIndicator.icon(ConnectionStatus.connected)),
    );
    expect(icon.color, scheme.primary);
    final op = tester.widget<FadeTransition>(fade()).opacity;
    expect(op.value, 1.0);
    await tester.pump(const Duration(milliseconds: 300));
    expect(op.value, 1.0, reason: 'connected does not animate');
  });

  // spec: app-ui/foreground-and-background
  testWidgets('Tap to retry: tapping while offline calls reconnect_now', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: bob);
    core.emit(
      const CoreEvent.connection(status: ConnectionStatus.disconnected),
    );
    await pumpIndicator(tester, core);
    await tester.tap(find.byTooltip('Offline'));
    await tester.pump();
    expect(core.calls, contains('reconnectNow'));
  });

  // spec: app-ui/foreground-and-background
  testWidgets('Tap to retry: tapping while connecting calls reconnect_now', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: bob);
    core.emit(const CoreEvent.connection(status: ConnectionStatus.connecting));
    await pumpIndicator(tester, core);
    await tester.tap(find.byTooltip('Connecting…'));
    await tester.pump();
    expect(core.calls, contains('reconnectNow'));
  });

  // spec: app-ui/foreground-and-background
  testWidgets('Nothing to retry: tapping while connected calls nothing', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: bob);
    core.emit(const CoreEvent.connection(status: ConnectionStatus.connected));
    await pumpIndicator(tester, core);
    await tester.tap(find.byTooltip('Connected'));
    await tester.pump();
    expect(core.calls, isNot(contains('reconnectNow')));
  });

  // spec: app-ui/foreground-and-background
  testWidgets('Nothing to retry: tapping while removed calls nothing and '
      'keeps the removed look', (tester) async {
    final core = FakeCoreService(storedUser: bob);
    core.emit(const CoreEvent.removed());
    core.emit(
      const CoreEvent.connection(status: ConnectionStatus.disconnected),
    );
    await pumpIndicator(tester, core);
    expect(find.byTooltip(ConnectionIndicator.removedLabel), findsOneWidget);
    expect(find.byIcon(ConnectionIndicator.removedIcon), findsOneWidget);
    await tester.tap(find.byIcon(ConnectionIndicator.removedIcon));
    await tester.pump();
    expect(core.calls, isNot(contains('reconnectNow')));
  });

  // spec: app-ui/foreground-and-background
  testWidgets('Pull to refresh: pulling the chat list down calls '
      'reconnect_now', (tester) async {
    // A DM is present, so the home screen is the chat list (GroupGate).
    final core = FakeCoreService(storedUser: bob)
      ..dmChats = [
        const ChatSummary(
          chatId: 1,
          kind: ChatKindInfo.dm(peer: akhil, waiting: false, closed: false),
          unread: 0,
          unseenMentions: 0,
          muted: false,
        ),
      ];
    await openApp(tester, core);
    expect(core.calls, ['open', 'start']);
    expect(find.byKey(const ValueKey('chat-main')), findsOneWidget);

    // Pull the list down past the refresh threshold.
    await tester.fling(
      find.byKey(const ValueKey('chat-main')),
      const Offset(0, 320),
      1000,
    );
    await tester.pump();
    await tester.pump(const Duration(seconds: 1));
    expect(core.calls, contains('reconnectNow'));

    // Let the refresh indicator finish so no timer outlives the test.
    await tester.pump(const Duration(seconds: 1));
  });

  // spec: app-ui/foreground-and-background
  testWidgets('color: offline is muted, connecting and connected active, '
      'removed is the error colour', (tester) async {
    final core = FakeCoreService(storedUser: bob);
    final scheme = await pumpIndicator(tester, core);
    expect(
      ConnectionIndicator.color(
        scheme,
        ConnectionStatus.connecting,
        removed: false,
      ),
      isNot(
        ConnectionIndicator.color(
          scheme,
          ConnectionStatus.disconnected,
          removed: false,
        ),
      ),
    );
    expect(
      ConnectionIndicator.color(
        scheme,
        ConnectionStatus.connected,
        removed: false,
      ),
      scheme.primary,
    );
    expect(
      ConnectionIndicator.color(
        scheme,
        ConnectionStatus.disconnected,
        removed: true,
      ),
      scheme.error,
    );
  });
}
