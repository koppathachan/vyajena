// Store screenshots (app-release: Store listing and icons; design D15). The
// app runs on the FAKE core with sample users and text, never a real group or
// a real Pi. Run with app/store/screenshots.sh <device id> <ios|android> on a
// throwaway simulator or emulator.
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:vyajena/app/app.dart';
import 'package:vyajena/core/core_service.dart';

import '../test/support/fake_core_service.dart';

const ammu = UserRef(username: 'ammu', serverId: 'home');
const appu = UserRef(username: 'appu', serverId: 'home');
const me = UserRef(username: 'akhil', serverId: 'home');

ChatMessage text(int id, UserRef from, String body, {bool mine = false}) =>
    ChatMessage(
      id: id,
      sender: from,
      senderDevice: mine ? 'aa11' : 'bb${id}2',
      outgoing: mine,
      ownUser: mine,
      body: ChatBody.text(text: body),
      mentions: const [],
      mentionsMe: false,
      acceptedAtMs: 1,
      status: mine ? const SendState.accepted() : null,
    );

/// The sample group: three people, a short family chat.
FakeCoreService sampleCore() => FakeCoreService(storedUser: me)
  ..admin = true
  ..group = 'Home'
  ..displayNames = {
    'ammu@home': 'Amma',
    'appu@home': 'Appu',
    'akhil@home': 'Akhil',
  }
  ..memberDevices = [
    const MemberDevice(user: me, device: 'aa11'),
    const MemberDevice(user: me, device: 'aa22'),
    const MemberDevice(user: ammu, device: 'bb12'),
    const MemberDevice(user: appu, device: 'cc13'),
  ]
  ..messages = [
    text(1, ammu, 'Did everyone reach home?'),
    text(2, appu, 'Yes, just now 🙂'),
    text(3, me, 'Reached. Traffic was bad near the bridge.', mine: true),
    text(4, ammu, 'Good. Lunch at ours on Sunday?'),
    text(5, appu, "I'll bring the payasam"),
    text(6, me, 'Count me in!', mine: true),
  ];

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  var converted = false;

  Future<void> shot(WidgetTester tester, String name) async {
    for (var i = 0; i < 10; i++) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    // Android draws to an image for screenshots; convert the surface once.
    if (Platform.isAndroid && !converted) {
      await binding.convertFlutterSurfaceToImage();
      converted = true;
    }
    await tester.pumpAndSettle();
    await binding.takeScreenshot(name);
  }

  // spec: app-release/store-listing-and-icons
  testWidgets('Screenshots show no real chat: the fake core\'s sample group '
      'only', (tester) async {
    final core = sampleCore();
    await tester.pumpWidget(VyajenaApp(core: core));
    await core.open();
    await shot(tester, '1-chat');

    await tester.tap(find.byTooltip('Menu'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Members'));
    await tester.pumpAndSettle();
    await shot(tester, '2-members');

    final joining = FakeCoreService();
    await tester.pumpWidget(const SizedBox());
    await tester.pumpWidget(VyajenaApp(core: joining));
    await joining.open();
    await shot(tester, '3-join');
  });
}
