// Opening the app over the fake core in widget tests.
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/app/app.dart';
import 'package:vyajena/core/core_service.dart';

import 'fake_core_service.dart';

const alice = UserRef(username: 'alice', serverId: 'pi');
const bob = UserRef(username: 'bob', serverId: 'pi');
const akhil = UserRef(username: 'akhil', serverId: 'pi');

/// Lets the fake core's futures, events and route transitions finish.
/// (Never `Future.delayed` in `testWidgets`: its fake clock never runs it.)
Future<void> settle(WidgetTester tester) async {
  for (var i = 0; i < 6; i++) {
    await tester.pump(const Duration(milliseconds: 100));
  }
}

/// Opens the app over [core], lets it settle, and lands in the main chat.
///
/// The app now always opens on the chat list (app-ui: Chat list); this helper
/// then opens the group's chat when there is no DM, so the many tests that
/// exercise the group chat land where they expect. Use [openHome] to stay on
/// the chat list.
Future<FakeCoreService> openApp(
  WidgetTester tester, [
  FakeCoreService? core,
]) async {
  final fake = await openHome(tester, core);
  if (fake.dmChats.isEmpty) {
    final group = find.byKey(const ValueKey('chat-main'));
    if (group.evaluate().isNotEmpty) await tap(tester, group);
  }
  return fake;
}

/// Opens the app over [core] and lets it settle, staying on the chat list.
Future<FakeCoreService> openHome(
  WidgetTester tester, [
  FakeCoreService? core,
]) async {
  final fake = core ?? FakeCoreService();
  await tester.pumpWidget(VyajenaApp(core: fake));
  await fake.open();
  await settle(tester);
  return fake;
}

/// Opens the group's chat from the chat list, for a test where the device
/// becomes in-group mid-test: home is the chat list now (app-ui: Create
/// affordance), so the group chat is one tap away rather than where the app
/// lands.
Future<void> enterGroup(WidgetTester tester) async {
  await tap(tester, find.byKey(const ValueKey('chat-main')));
}

/// Taps [finder] and settles.
Future<void> tap(WidgetTester tester, Finder finder) async {
  await tester.ensureVisible(finder);
  await tester.tap(finder);
  await settle(tester);
}

/// Opens the chat menu and picks [label].
Future<void> menu(WidgetTester tester, String label) async {
  await tap(tester, find.byTooltip('Menu'));
  await tap(tester, find.text(label));
}

/// Records what the app asks the platform to share.
List<String> recordShares(WidgetTester tester) {
  final shared = <String>[];
  tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
    const MethodChannel('vyajena/platform'),
    (call) async {
      if (call.method == 'share') {
        shared.add((call.arguments as Map)['text'] as String);
      }
      return null;
    },
  );
  addTearDown(
    () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      const MethodChannel('vyajena/platform'),
      null,
    ),
  );
  return shared;
}
