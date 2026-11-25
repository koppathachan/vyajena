// app-ui: Create a sticker (change `photo-sticker`): pick a photo, cut the
// subject out on the device, preview the transparent result, send it as an
// image; and render a received transparent image without an opaque bubble fill.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/chat/composer.dart';
import 'package:vyajena/chat/sticker_preview.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';
import '../support/fake_sticker.dart';

final attach = find.byKey(const Key('attach-image'));
final stickerItem = find.byKey(const Key('attach-sticker'));
final stickerBar = find.byKey(const Key('sticker-preview-bar'));
final sendSticker = find.byKey(const Key('send-sticker'));

/// Opens the attachment sheet and taps Sticker.
Future<void> chooseSticker(WidgetTester tester) async {
  await tap(tester, attach);
  await tap(tester, stickerItem);
}

FakeCoreService group() => FakeCoreService(storedUser: bob)
  ..memberDevices = const [
    MemberDevice(user: akhil, device: '01'),
    MemberDevice(user: bob, device: '03'),
  ];

void main() {
  // spec: app-ui/create-a-sticker
  testWidgets('pick, preview and send a sticker', (tester) async {
    final core = group();
    final picker = FakePicker(FakeCoreService.blurPng);
    final segmenter = FakeSegmenter();
    await openAppWithStickers(tester, core, picker, segmenter);

    await chooseSticker(tester);
    // The photo was picked and the subject cut out on the device.
    expect(picker.opened, 1);
    expect(segmenter.cutouts, 1);
    // The transparent result is previewed over a checkerboard.
    expect(stickerBar, findsOneWidget);
    expect(find.byType(StickerPreview), findsOneWidget);
    // Nothing has been sent yet.
    expect(core.sentStickers, isEmpty);

    await tap(tester, sendSticker);
    // The app handed the RGBA cutout to the core, which sends it; the app never
    // encoded or uploaded it (no sendImage call).
    expect(core.calls.contains('sendSticker'), isTrue);
    expect(core.calls.contains('sendImage'), isFalse);
    expect(core.sentStickers, hasLength(1));
    expect(core.sentStickers.single.source, StickerSource.iosVision);
    // The preview is gone after sending.
    expect(stickerBar, findsNothing);
  });

  // spec: app-ui/create-a-sticker
  testWidgets('no subject found tells the user and sends nothing', (
    tester,
  ) async {
    final core = group();
    final picker = FakePicker(FakeCoreService.blurPng);
    final segmenter = FakeSegmenter(result: Outcome.noSubject);
    await openAppWithStickers(tester, core, picker, segmenter);

    await chooseSticker(tester);
    expect(find.text(noSubjectText), findsOneWidget);
    expect(stickerBar, findsNothing);
    expect(core.sentStickers, isEmpty);
  });

  // spec: app-ui/create-a-sticker
  testWidgets('an unsupported OS tells the user and sends nothing', (
    tester,
  ) async {
    final core = group();
    final picker = FakePicker(FakeCoreService.blurPng);
    final segmenter = FakeSegmenter(result: Outcome.unsupported);
    await openAppWithStickers(tester, core, picker, segmenter);

    await chooseSticker(tester);
    expect(find.text(stickerUnsupportedText), findsOneWidget);
    expect(core.sentStickers, isEmpty);
  });

  // spec: app-ui/create-a-sticker
  testWidgets('a received sticker renders without an opaque bubble fill', (
    tester,
  ) async {
    final core = group();
    await openApp(tester, core);
    core.receiveSticker(akhil);
    await settle(tester);

    final bubble = find.byKey(const Key('image-bubble'));
    expect(bubble, findsOneWidget);
    // The message's card has no opaque fill, so the transparency reads.
    final card = tester.widget<Card>(
      find.ancestor(of: bubble, matching: find.byType(Card)),
    );
    expect(card.color, Colors.transparent);
  });

  // spec: app-ui/create-a-sticker
  testWidgets('an ordinary opaque image keeps its bubble fill', (tester) async {
    final core = group();
    await openApp(tester, core);
    // A normal image (not a sticker): its thumbhash is not transparent.
    core.receiveImage(akhil);
    await settle(tester);

    final bubble = find.byKey(const Key('image-bubble'));
    final card = tester.widget<Card>(
      find.ancestor(of: bubble, matching: find.byType(Card)),
    );
    expect(card.color, isNot(Colors.transparent));
  });
}
