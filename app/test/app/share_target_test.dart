// app-ui: Vyajena is a share target (change `message-actions-share`). Inbound
// shared text opens the chat picker and composes it in the chosen chat; a share
// before onboarding is ignored with a notice. The native intent/extension
// plumbing is verified manually (see the change); here the Dart intake is driven
// with a payload directly.
import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/app/platform.dart';
import 'package:vyajena/app/share_intake.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

FakeCoreService group() => FakeCoreService(storedUser: alice)
  ..memberDevices = const [
    MemberDevice(user: alice, device: '02'),
    MemberDevice(user: bob, device: '03'),
  ];

/// Pumps a bare app with the keys the intake drives, over [core].
Future<ShareIntake> pumpIntake(
  WidgetTester tester,
  FakeCoreService core,
) async {
  reduceMotion(tester);
  final navigatorKey = GlobalKey<NavigatorState>();
  final messengerKey = GlobalKey<ScaffoldMessengerState>();
  await core.open();
  await tester.pumpWidget(
    MaterialApp(
      navigatorKey: navigatorKey,
      scaffoldMessengerKey: messengerKey,
      home: const Scaffold(body: SizedBox.shrink()),
    ),
  );
  return ShareIntake(
    core: core,
    navigatorKey: navigatorKey,
    messengerKey: messengerKey,
  );
}

void main() {
  // spec: app-ui/vyajena-is-a-share-target
  testWidgets('shared text is composed into the chosen chat', (tester) async {
    final core = group()..dmChats = [];
    final intake = await pumpIntake(tester, core);

    unawaited(intake.deliver(const SharedText('look at this')));
    await settle(tester);

    // The picker opens; choose the lobby (chat 0).
    expect(find.byKey(const Key('forward-picker')), findsOneWidget);
    await tester.tap(find.byKey(const Key('forward-to-0')));
    await settle(tester);

    expect(core.calls, contains('sendText'));
    expect(core.sent, contains('look at this'));
    expect(find.text('Sent to lobby'), findsOneWidget);
  });

  // spec: app-ui/vyajena-is-a-share-target
  testWidgets('a shared image is sent as a new attachment', (tester) async {
    final core = group();
    final intake = await pumpIntake(tester, core);
    final bytes = Uint8List.fromList([9, 8, 7, 6]);

    unawaited(intake.deliver(SharedImage(bytes)));
    await settle(tester);

    await tester.tap(find.byKey(const Key('forward-to-0')));
    await settle(tester);

    expect(core.calls, contains('sendImage'));
    expect(core.sentPictures, contains(bytes));
  });

  // spec: app-ui/vyajena-is-a-share-target
  testWidgets('a share before onboarding is ignored with a notice', (
    tester,
  ) async {
    // No stored user: the core opens unregistered.
    final core = FakeCoreService();
    final intake = await pumpIntake(tester, core);

    unawaited(intake.deliver(const SharedText('hi')));
    await settle(tester);

    expect(find.text(shareBeforeOnboardingText), findsOneWidget);
    expect(find.byKey(const Key('forward-picker')), findsNothing);
    expect(core.calls, isNot(contains('sendText')));
  });
}
