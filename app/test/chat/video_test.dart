// app-ui: Send a video; Video bubble and player (change `video-messages`).
import 'package:flutter/material.dart';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/chat/video_bubble.dart';
import 'package:vyajena/chat/video_plugins.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';
import '../support/fake_video.dart';

/// Bob's device in a group with akhil and alice.
FakeCoreService groupAsBob() =>
    FakeCoreService(storedUser: bob)
      ..memberDevices = const [
        MemberDevice(user: akhil, device: '01'),
        MemberDevice(user: alice, device: '02'),
        MemberDevice(user: bob, device: '03'),
      ];

final attach = find.byKey(const Key('attach-image'));
final bubble = find.byKey(const Key('video-bubble'));
final send = find.byTooltip('Send');

/// The record argument of [FakeCoreService.sendVideo], as the fake stores it.
typedef SendVideoCall = ({
  int chatId,
  int durationMs,
  String? caption,
  List<MentionSpan> mentions,
});

/// Opens the attachment sheet, chooses Video and a source: the clip is
/// normalized and staged in the compose tray (not yet sent).
Future<void> attachVideo(WidgetTester tester, String source) async {
  await tap(tester, attach);
  await tap(tester, find.byKey(const Key('attach-video')));
  await tap(tester, find.byKey(Key('video-$source')));
}

/// Stages a video from [source] and taps Send.
Future<void> attachAndSendVideo(WidgetTester tester, String source) async {
  await attachVideo(tester, source);
  await settle(tester);
  await tap(tester, send);
}

void main() {
  // spec: app-ui/send-a-video
  testWidgets('Bob records a clip: the app normalizes it and hands the core '
      'the normalized bytes, which show as a video bubble', (tester) async {
    final core = groupAsBob();
    final video = FakeVideoSource();
    await openAppWithVideo(tester, core, video);

    await attachAndSendVideo(tester, 'record');

    expect(video.recordCalls, 1);
    expect(core.calls, contains('sendVideo'));
    final call = core.args['sendVideo']! as SendVideoCall;
    expect(call.chatId, 0);
    expect(call.durationMs, 12000);
    // The app did no transcode: the plugin's one normalized clip went to the
    // core as it was.
    expect(core.sentVideos.length, 1);
    expect(core.sent, isEmpty, reason: 'it was not sent as text');
    // The message is in the chat as a video bubble.
    expect(bubble, findsOneWidget);
  });

  // spec: app-ui/send-a-video
  testWidgets('Bob picks a clip from the gallery and it is sent', (
    tester,
  ) async {
    final core = groupAsBob();
    final video = FakeVideoSource();
    await openAppWithVideo(tester, core, video);

    await attachAndSendVideo(tester, 'gallery');

    expect(video.pickCalls, 1);
    expect(core.calls, contains('sendVideo'));
    expect(bubble, findsOneWidget);
  });

  // spec: app-ui/send-a-video
  testWidgets('sending a video while replying clears the reply bar', (
    tester,
  ) async {
    final core = groupAsBob();
    await openAppWithVideo(tester, core, FakeVideoSource());
    core.receive(alice, 'dinner at 8?');
    await settle(tester);
    await tester.drag(find.text('dinner at 8?'), const Offset(300, 0));
    await settle(tester);
    expect(find.byKey(const Key('reply-bar')), findsOneWidget);

    await attachAndSendVideo(tester, 'record');

    expect(core.calls, contains('sendVideo'));
    expect(find.byKey(const Key('reply-bar')), findsNothing);
  });

  // spec: app-ui/send-a-video
  testWidgets('backing out of the picker sends nothing', (tester) async {
    final core = groupAsBob();
    final video = FakeVideoSource(backsOut: true);
    await openAppWithVideo(tester, core, video);

    await attachVideo(tester, 'gallery');
    await settle(tester);

    expect(video.pickCalls, 1);
    expect(core.calls, isNot(contains('sendVideo')));
    expect(bubble, findsNothing);
  });

  // spec: app-ui/video-bubble-and-player
  testWidgets('a received video shows its blur at its aspect ratio with a play '
      'overlay and duration; a tap downloads it then opens the player', (
    tester,
  ) async {
    final core = groupAsBob();
    final message = core.receiveVideo(alice);
    String? opened;
    final video = message.body as ChatBody_Video;

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: VideoBubble(
            core: core,
            messageId: message.id,
            video: video.video,
            openViewer: (context, path) async => opened = path,
          ),
        ),
      ),
    );
    await settle(tester);

    expect(find.byKey(const Key('video-blur')), findsOneWidget);
    expect(find.byKey(const Key('video-play')), findsOneWidget);
    // The duration (0:12) and size are shown before anything downloads.
    expect(find.textContaining('0:12'), findsOneWidget);
    final size = tester.getSize(bubble);
    expect(size.width / size.height, closeTo(854 / 480, 0.01));
    expect(core.calls, isNot(contains('downloadVideo')));

    await tap(tester, bubble);
    await settle(tester);
    // The clip downloaded through the core, and the player opened on its path.
    expect(core.args['downloadVideo'], message.id);
    expect(opened, '/media/clip-${message.id}.mp4');
  });

  // spec: app-ui/downloaded-video-shows-first-frame
  testWidgets('a downloaded video shows its first frame; before that only the '
      'blur (nothing downloads by itself)', (tester) async {
    final core = groupAsBob();
    final message = core.receiveVideo(alice);
    final video = message.body as ChatBody_Video;
    Widget bubbleWidget() => MaterialApp(
      home: Scaffold(
        body: VideoBubble(
          core: core,
          messageId: message.id,
          video: video.video,
          openViewer: (context, path) async {},
          firstFrame: (context, path) =>
              Text(path, key: const Key('fake-frame')),
        ),
      ),
    );

    // Not in the cache: the blur, no frame, and no download.
    await tester.pumpWidget(bubbleWidget());
    await settle(tester);
    expect(find.byKey(const Key('video-first-frame')), findsNothing);
    expect(core.calls, isNot(contains('downloadVideo')));

    // Tap: it downloads, and the first frame appears in the bubble.
    await tap(tester, find.byKey(const Key('video-bubble')));
    await settle(tester);
    expect(find.byKey(const Key('video-first-frame')), findsOneWidget);
    expect(find.text('/media/clip-${message.id}.mp4'), findsOneWidget);

    // Already in the cache when the bubble is built: the frame shows at once,
    // without a tap and without downloading again.
    core.calls.clear();
    await tester.pumpWidget(const SizedBox());
    await tester.pumpWidget(bubbleWidget());
    await settle(tester);
    expect(find.byKey(const Key('video-first-frame')), findsOneWidget);
    expect(core.calls, isNot(contains('downloadVideo')));
  });

  // spec: app-ui/video-bubble-and-player
  testWidgets('an expired video says so and downloads nothing', (tester) async {
    final core = groupAsBob();
    final message = core.receiveVideo(alice, expired: true);
    final video = message.body as ChatBody_Video;

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: VideoBubble(
            core: core,
            messageId: message.id,
            video: video.video,
            openViewer: (context, path) async {},
          ),
        ),
      ),
    );
    await settle(tester);

    expect(find.byKey(const Key('video-expired')), findsOneWidget);
    expect(find.text(videoExpiredText), findsOneWidget);
    await tap(tester, bubble);
    await settle(tester);
    expect(core.calls, isNot(contains('downloadVideo')));
  });

  // spec: app-ui/video-bubble-and-player
  testWidgets('the sender can resend an own video from the long-press menu', (
    tester,
  ) async {
    final core = groupAsBob();
    final video = FakeVideoSource();
    await openAppWithVideo(tester, core, video);
    await attachAndSendVideo(tester, 'record');

    // Long-press the own video bubble → Resend video.
    await tester.longPress(bubble);
    await settle(tester);
    expect(find.byKey(const Key('resend-video')), findsOneWidget);
    await tap(tester, find.byKey(const Key('resend-video')));
    await settle(tester);
    expect(core.calls, contains('resendVideo'));
  });

  // spec: app-ui/compose-tray
  testWidgets('picking several clips stages them in the tray, then one Send '
      'sends each as its own message', (tester) async {
    final core = groupAsBob();
    final video = FakeVideoSource()..many = [_clip(1000), _clip(2000)];
    await openAppWithVideo(tester, core, video);

    await attachVideo(tester, 'gallery');
    await settle(tester);

    expect(video.multiCalls, 1);
    // Both stage; nothing is sent until Send.
    expect(find.byKey(const Key('staged-0')), findsOneWidget);
    expect(find.byKey(const Key('staged-1')), findsOneWidget);
    expect(core.calls, isNot(contains('sendVideo')));

    await tap(tester, send);
    // Two clips, each sent as its own video message.
    expect(core.sentVideos.length, 2);
    expect(bubble, findsNWidgets(2));
  });
}

/// A normalized clip of [durationMs] for the multi-pick test.
NormalizedVideo _clip(int durationMs) => NormalizedVideo(
  mp4: Uint8List.fromList([0, 0, 0, 0x18, ...'ftyp'.codeUnits, 1, 2, 3, 4]),
  durationMs: durationMs,
  width: 854,
  height: 480,
  firstFrameRgba: Uint8List.fromList(List<int>.filled(16 * 16 * 4, 100)),
  frameWidth: 16,
  frameHeight: 16,
);
