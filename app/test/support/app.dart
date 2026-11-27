// Opening the app over the fake core in widget tests.
import 'package:flutter/material.dart' show ListTile;
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/app/app.dart';
import 'package:vyajena/app/theme_mode.dart';
import 'package:vyajena/chat/picture_picker.dart';
import 'package:vyajena/chat/sticker_plugins.dart';
import 'package:vyajena/chat/video_plugins.dart';
import 'package:vyajena/chat/voice_plugins.dart';
import 'package:vyajena/core/core_service.dart';

import 'fake_core_service.dart';

const alice = UserRef(username: 'alice', serverId: 'chat');
const bob = UserRef(username: 'bob', serverId: 'chat');
const akhil = UserRef(username: 'akhil', serverId: 'chat');

/// Turns the phone's reduce-motion setting on for this test.
void reduceMotion(WidgetTester tester) {
  tester.platformDispatcher.accessibilityFeaturesTestValue =
      const FakeAccessibilityFeatures(disableAnimations: true);
  addTearDown(tester.platformDispatcher.clearAccessibilityFeaturesTestValue);
}

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
  if (fake.dmChats.isEmpty && fake.groupChats.isEmpty) {
    final group = find.byKey(const ValueKey('chat-main'));
    if (group.evaluate().isNotEmpty) await tap(tester, group);
  }
  return fake;
}

/// A gallery that gives what a test sets: [next] the first time, then nothing.
class FakePicker extends PicturePicker {
  FakePicker([this.next]);

  /// What the user picks next; null is "chose nothing".
  Uint8List? next;

  /// How many times the gallery was opened.
  int opened = 0;

  @override
  Future<Uint8List?> pick() async {
    opened++;
    final picked = next;
    next = null;
    return picked;
  }
}

/// [openApp] with [pictures] as the gallery (app-ui: Send an image).
Future<FakeCoreService> openAppWithPictures(
  WidgetTester tester,
  FakeCoreService core,
  PicturePicker pictures,
) async {
  await tester.pumpWidget(VyajenaApp(core: core, pictures: pictures));
  await core.open();
  await settle(tester);
  final group = find.byKey(const ValueKey('chat-main'));
  if (core.dmChats.isEmpty &&
      core.groupChats.isEmpty &&
      group.evaluate().isNotEmpty) {
    await tap(tester, group);
  }
  return core;
}

/// [openApp] with [pictures] as the gallery and [segmenter] as the on-device
/// subject segmentation (app-ui: Create a sticker), landing in the group chat.
Future<FakeCoreService> openAppWithStickers(
  WidgetTester tester,
  FakeCoreService core,
  PicturePicker pictures,
  Segmenter segmenter,
) async {
  await tester.pumpWidget(
    VyajenaApp(core: core, pictures: pictures, segmenter: segmenter),
  );
  await core.open();
  await settle(tester);
  final group = find.byKey(const ValueKey('chat-main'));
  if (core.dmChats.isEmpty &&
      core.groupChats.isEmpty &&
      group.evaluate().isNotEmpty) {
    await tap(tester, group);
  }
  return core;
}

/// [openApp] with [voice] as the microphone and speaker (app-ui: Record a voice
/// message; Play a voice message).
Future<FakeCoreService> openAppWithVoice(
  WidgetTester tester,
  FakeCoreService core,
  VoicePlugins voice,
) async {
  await tester.pumpWidget(VyajenaApp(core: core, voice: voice));
  await core.open();
  await settle(tester);
  final group = find.byKey(const ValueKey('chat-main'));
  if (core.dmChats.isEmpty && group.evaluate().isNotEmpty) {
    await tap(tester, group);
  }
  return core;
}

/// [openApp] with [video] as the camera/gallery and native normalize plugin
/// (app-ui: Send a video).
Future<FakeCoreService> openAppWithVideo(
  WidgetTester tester,
  FakeCoreService core,
  VideoSource video,
) async {
  await tester.pumpWidget(VyajenaApp(core: core, video: video));
  await core.open();
  await settle(tester);
  final group = find.byKey(const ValueKey('chat-main'));
  if (core.dmChats.isEmpty && group.evaluate().isNotEmpty) {
    await tap(tester, group);
  }
  return core;
}

/// Opens the app over [core] and lets it settle, staying on the chat list.
Future<FakeCoreService> openHome(
  WidgetTester tester, [
  FakeCoreService? core,
]) async {
  final fake = core ?? FakeCoreService();
  // Arcade motion: no animation of its own under reduce-motion, so a looping
  // float never holds a pump-until-settled test up. Motion has its own tests.
  reduceMotion(tester);
  await tester.pumpWidget(
    VyajenaApp(core: fake, themeStore: MemoryThemeModeStore()),
  );
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
  // Whatever was just typed or set has been drawn (the composer's button is
  // the mic until there is text).
  await tester.pump();
  await tester.ensureVisible(finder);
  await tester.tap(finder);
  await settle(tester);
}

/// Opens the chat menu and picks [label].
Future<void> menu(WidgetTester tester, String label) async {
  await tap(tester, find.byTooltip('Menu'));
  await tap(tester, find.text(label));
}

/// Opens an action on the Contacts tab by its [key] (app-shell: Contacts tab):
/// e.g. 'contacts-invite', 'contacts-invites', 'contacts-new-group',
/// 'contacts-remove-user'. The app must be on the shell (open it with
/// [openHome]), since the admin actions moved off the old chat overflow menu to
/// this tab (change `shell-nav-consistency`).
Future<void> contactsAction(WidgetTester tester, String key) async {
  await tap(tester, find.byKey(const Key('tab-contacts')));
  await tap(tester, find.byKey(Key(key)));
}

/// Opens the Profile tab (app-shell), where Link a new device and the profile
/// settings now live (change `shell-nav-consistency`).
Future<void> openProfileTab(WidgetTester tester) async {
  await tap(tester, find.byKey(const Key('tab-profile')));
}

/// A tall screen, so a whole long tab (e.g. the Profile tab, whose settings sit
/// below the fold) is built and reachable in a lazy ListView.
void tall(WidgetTester tester) {
  tester.view.physicalSize = const Size(800, 2400);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
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

/// The name a chat-list row shows: its title Text, or (the lobby, a muted row)
/// the Text inside the title's Row.
String? tileTitle(ListTile tile) {
  final title = tile.title;
  if (title is Text) return title.data;
  if (title is Row) {
    for (final child in title.children) {
      final widget = child is Flexible ? child.child : child;
      if (widget is Text) return widget.data;
    }
  }
  return null;
}
