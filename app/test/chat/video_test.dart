// app-ui: Send a video; Video bubble and player (change `video-messages`).
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/chat/video_bubble.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';
import '../support/fake_video.dart';

/// Bob's device in a group with akhil and alice.
FakeCoreService groupAsBob() => FakeCoreService(storedUser: bob)
  ..memberDevices = const [
    MemberDevice(user: akhil, device: '01'),
    MemberDevice(user: alice, device: '02'),
    MemberDevice(user: bob, device: '03'),
  ];

final attach = find.byKey(const Key('attach-image'));
final bubble = find.byKey(const Key('video-bubble'));

/// Opens the attachment sheet and chooses Video, then a source.
Future<void> attachVideo(WidgetTester tester, String source) async {
  await tap(tester, attach);
  await tap(tester, find.byKey(const Key('attach-video')));
  await tap(tester, find.byKey(Key('video-$source')));
}

void main() {
  // spec: app-ui/send-a-video
  testWidgets('Bob records a clip: the app normalizes it and hands the core '
      'the normalized bytes, which show as a video bubble', (tester) async {
    final core = groupAsBob();
    final video = FakeVideoSource();
    await openAppWithVideo(tester, core, video);

    await attachVideo(tester, 'record');
    await settle(tester);

    expect(video.recordCalls, 1);
    expect(core.calls, contains('sendVideo'));
    final call = core.args['sendVideo']! as ({int chatId, int durationMs});
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

    await attachVideo(tester, 'gallery');
    await settle(tester);

    expect(video.pickCalls, 1);
    expect(core.calls, contains('sendVideo'));
    expect(bubble, findsOneWidget);
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
    await attachVideo(tester, 'record');
    await settle(tester);

    // Long-press the own video bubble → Resend video.
    await tester.longPress(bubble);
    await settle(tester);
    expect(find.byKey(const Key('resend-video')), findsOneWidget);
    await tap(tester, find.byKey(const Key('resend-video')));
    await settle(tester);
    expect(core.calls, contains('resendVideo'));
  });
}
