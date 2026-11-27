// app-ui: Compose tray; Video send progress; Video caption shown in the bubble
// (change `media-compose-tray`).
import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/app/app.dart';
import 'package:vyajena/chat/picture_picker.dart';
import 'package:vyajena/chat/video_plugins.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';
import '../support/fake_video.dart';

/// Bob's device in a group with akhil and alice, where alice shows as "Ali".
FakeCoreService groupAsBob() => FakeCoreService(storedUser: bob)
  ..memberDevices = const [
    MemberDevice(user: akhil, device: '01'),
    MemberDevice(user: alice, device: '02'),
    MemberDevice(user: bob, device: '03'),
  ]
  ..displayNames = {
    'alice@chat': 'Ali (alice)',
    'bob@chat': 'Bobby (bob)',
    'akhil@chat': 'akhil',
  }
  ..profiles = {
    'alice@chat': const ProfileInfo(displayName: 'Ali'),
    'bob@chat': const ProfileInfo(displayName: 'Bobby'),
  };

final attach = find.byKey(const Key('attach-image'));
final send = find.byTooltip('Send');
final composer = find.byKey(const Key('composer'));
final videoBubble = find.byKey(const Key('video-bubble'));

/// The record argument of [FakeCoreService.sendVideo], as the fake stores it.
typedef SendVideoCall = ({
  int chatId,
  int durationMs,
  String? caption,
  List<MentionSpan> mentions,
});

/// The record argument of [FakeCoreService.sendImage].
typedef SendImageCall = ({int chatId, String? caption, List<MentionSpan> mentions});

/// Opens the app over [core] with both a gallery and a video source, landing in
/// the group chat.
Future<FakeCoreService> openWithMedia(
  WidgetTester tester,
  FakeCoreService core, {
  required PicturePicker pictures,
  required VideoSource video,
}) async {
  await tester.pumpWidget(
    VyajenaApp(core: core, pictures: pictures, video: video),
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

/// A video source whose pick does not finish until the test lets it, so the
/// "processing" state can be observed (app-ui: Video send progress).
class SlowVideoSource implements VideoSource {
  final _gate = Completer<NormalizedVideo?>();

  void finish(NormalizedVideo clip) => _gate.complete(clip);

  @override
  Future<NormalizedVideo?> record() => _gate.future;

  @override
  Future<NormalizedVideo?> pick() => _gate.future;

  @override
  Future<List<NormalizedVideo>> pickMultiple() async {
    final one = await pick();
    return one == null ? const [] : [one];
  }
}

NormalizedVideo sampleClip() => NormalizedVideo(
  mp4: Uint8List.fromList([0, 0, 0, 0x18, ...'ftyp'.codeUnits, 1, 2, 3, 4]),
  durationMs: 9000,
  width: 854,
  height: 480,
  firstFrameRgba: Uint8List.fromList(List<int>.filled(16 * 16 * 4, 100)),
  frameWidth: 16,
  frameHeight: 16,
);

/// The rich text span drawn for [text] anywhere on screen.
TextSpan? spanOf(WidgetTester tester, String text) {
  for (final widget in tester.widgetList<RichText>(find.byType(RichText))) {
    TextSpan? found;
    widget.text.visitChildren((span) {
      if (span is TextSpan && span.text == text) {
        found = span;
        return false;
      }
      return true;
    });
    if (found != null) return found;
  }
  return null;
}

const mentionOfBob = MentionSpan(
  target: MentionTarget.user(user: bob),
  start: 0,
  length: 4,
);

void main() {
  // spec: app-ui/compose-tray
  testWidgets('a mixed batch of photos and a video sends as separate messages '
      'in order, the one caption on the last item', (tester) async {
    final core = groupAsBob();
    final one = Uint8List.fromList([1, 2, 3, 4]);
    final two = Uint8List.fromList([5, 6, 7, 8]);
    final picker = FakePicker()..many = [one, two];
    final video = FakeVideoSource();
    await openWithMedia(tester, core, pictures: picker, video: video);

    // Add two photos.
    await tap(tester, attach);
    await tap(tester, find.byKey(const Key('attach-photo')));
    await tap(tester, find.byKey(const Key('photo-gallery')));
    expect(find.byKey(const Key('staged-0')), findsOneWidget);
    expect(find.byKey(const Key('staged-1')), findsOneWidget);

    // Tap + and add a video from the gallery.
    await tap(tester, find.byKey(const Key('tray-add')));
    await tap(tester, find.byKey(const Key('attach-video')));
    await tap(tester, find.byKey(const Key('video-gallery')));
    await settle(tester);
    expect(find.byKey(const Key('staged-2')), findsOneWidget);

    // One caption with a mention, then one Send.
    await tester.enterText(composer, '@al');
    await tester.pump();
    await tap(tester, find.text('Ali (alice)'));
    await tap(tester, send);

    // Three separate messages in order: the two photos captionless, then the
    // video carrying the caption with the @Ali mention.
    expect(core.sentPictures, [one, two]);
    expect(core.sentVideos.length, 1);
    final vcall = core.args['sendVideo']! as SendVideoCall;
    expect(vcall.caption, '@Ali ');
    expect(vcall.mentions.single.target, MentionTarget.user(user: alice));
    final icall = core.args['sendImage']! as SendImageCall;
    expect(icall.caption, isNull, reason: 'earlier items are captionless');
    // The tray is cleared and the composer is ready for the next message.
    expect(find.byKey(const Key('compose-tray')), findsNothing);
  });

  // spec: app-ui/compose-tray
  testWidgets('removing the only staged item dismisses the tray and sends '
      'nothing', (tester) async {
    final core = groupAsBob();
    final picker = FakePicker(Uint8List.fromList([9, 9]));
    await openWithMedia(tester, core, pictures: picker, video: FakeVideoSource());

    await tap(tester, attach);
    await tap(tester, find.byKey(const Key('attach-photo')));
    await tap(tester, find.byKey(const Key('photo-gallery')));
    expect(find.byKey(const Key('compose-tray')), findsOneWidget);

    await tap(tester, find.byKey(const Key('remove-staged-0')));
    expect(find.byKey(const Key('compose-tray')), findsNothing);
    expect(core.calls, isNot(contains('sendImage')));
  });

  // spec: app-ui/video-send-progress
  testWidgets('a video being normalized shows "Processing video…" and Send is '
      'disabled until it is ready', (tester) async {
    final core = groupAsBob();
    final video = SlowVideoSource();
    await openWithMedia(
      tester,
      core,
      pictures: const GalleryPicker(),
      video: video,
    );

    await tap(tester, attach);
    await tap(tester, find.byKey(const Key('attach-video')));
    await tap(tester, find.byKey(const Key('video-gallery')));
    await tester.pump();

    // The normalize is in flight: the processing tile shows and Send is off.
    final sendButton = find.byKey(const Key('composer-send'));
    expect(find.byKey(const Key('tray-processing')), findsOneWidget);
    expect(find.text('Processing video…'), findsOneWidget);
    expect(tester.widget<IconButton>(sendButton).onPressed, isNull);
    expect(core.calls, isNot(contains('sendVideo')));

    // When the clip is ready the tile goes, the thumbnail appears and Send is on.
    video.finish(sampleClip());
    await settle(tester);
    expect(find.byKey(const Key('tray-processing')), findsNothing);
    expect(find.byKey(const Key('staged-0')), findsOneWidget);
    expect(tester.widget<IconButton>(sendButton).onPressed, isNotNull);
  });

  // spec: app-ui/video-caption-shown-in-the-bubble
  testWidgets('a received video shows its caption with the mention highlighted',
      (tester) async {
    final core = groupAsBob();
    await openWithMedia(
      tester,
      core,
      pictures: const GalleryPicker(),
      video: FakeVideoSource(),
    );
    core.receiveVideo(alice, caption: '@Bob look', mentions: [mentionOfBob]);
    await settle(tester);

    expect(videoBubble, findsOneWidget);
    // Bob is "Bobby" to himself; the caption names him, so it stands out.
    final mention = spanOf(tester, '@Bobby');
    expect(mention, isNotNull);
    expect(mention!.style?.fontWeight, FontWeight.bold);
    expect(spanOf(tester, ' look'), isNotNull);
  });

  // spec: app-ui/video-caption-shown-in-the-bubble
  testWidgets('a captionless video shows no caption line', (tester) async {
    final core = groupAsBob();
    await openWithMedia(
      tester,
      core,
      pictures: const GalleryPicker(),
      video: FakeVideoSource(),
    );
    core.receiveVideo(alice);
    await settle(tester);

    expect(videoBubble, findsOneWidget);
    expect(spanOf(tester, ' look'), isNull);
    expect(find.textContaining('@'), findsNothing);
  });
}
