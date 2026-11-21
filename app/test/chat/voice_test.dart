// app-ui: Record a voice message; Play a voice message (change `audio-messages`).
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/chat/audio_bubble.dart';
import 'package:vyajena/chat/composer.dart';
import 'package:vyajena/chat/message_tile.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';
import '../support/fake_voice.dart';

FakeCoreService groupAsBob() => FakeCoreService(storedUser: bob)
  ..memberDevices = const [
    MemberDevice(user: akhil, device: '01'),
    MemberDevice(user: alice, device: '02'),
    MemberDevice(user: bob, device: '03'),
  ];

final mic = find.byKey(const Key('record-voice'));
final recordingBar = find.byKey(const Key('recording-bar'));
final voicePreview = find.byKey(const Key('voice-preview'));
final sendVoice = find.byKey(const Key('send-voice'));
final deleteVoice = find.byKey(const Key('preview-delete'));
final composer = find.byKey(const Key('composer'));
final play = find.byKey(const Key('audio-play'));

/// Holds the mic for [held], then lets go (or slides first, to cancel).
Future<void> record(
  WidgetTester tester, {
  Duration held = const Duration(seconds: 2),
  double slide = 0,
}) async {
  final gesture = await tester.startGesture(tester.getCenter(mic));
  await tester.pump(const Duration(milliseconds: 50));
  await tester.pump(held);
  if (slide != 0) {
    await gesture.moveBy(Offset(-slide, 0));
    await tester.pump();
  }
  await gesture.up();
  await settle(tester);
}

void main() {
  group('record a voice message', () {
    // spec: app-ui/record-a-voice-message
    testWidgets('Bob holds the mic, releases and sends from the preview: the '
        'core gets the recorded file as it is, and the file is deleted', (
      tester,
    ) async {
      final core = groupAsBob();
      final voice = FakeVoicePlugins()
        ..contents = Uint8List.fromList([9, 8, 7, 6, 5]);
      await openAppWithVoice(tester, core, voice);
      expect(mic, findsOneWidget);

      final gesture = await tester.startGesture(tester.getCenter(mic));
      await tester.pump(const Duration(milliseconds: 50));
      expect(voice.permissionAsked, 1);
      expect(voice.started, 1);
      expect(recordingBar, findsOneWidget);
      await tester.pump(const Duration(seconds: 3));
      expect(find.text('0:03'), findsOneWidget);
      await gesture.up();
      await settle(tester);

      // The preview offers play, delete and send; nothing is sent yet.
      expect(voice.stopped, 1);
      expect(voicePreview, findsOneWidget);
      expect(core.calls, isNot(contains('sendAudio')));

      await tap(tester, sendVoice);
      // The core got the recorder's bytes, untouched: the app encodes nothing.
      expect(core.sentRecordings.single, [9, 8, 7, 6, 5]);
      expect(File(voice.lastPath!).existsSync(), isFalse);
      expect(voicePreview, findsNothing);
      // Shown as sending until the core accepts it.
      expect(find.byKey(const Key('audio-bubble')), findsOneWidget);
      expect(find.text('⏳'), findsOneWidget);
    });

    // spec: app-ui/record-a-voice-message
    testWidgets('sliding left while holding cancels: nothing is kept or sent', (
      tester,
    ) async {
      final core = groupAsBob();
      final voice = FakeVoicePlugins();
      await openAppWithVoice(tester, core, voice);

      final gesture = await tester.startGesture(tester.getCenter(mic));
      await tester.pump(const Duration(seconds: 1));
      expect(find.text('‹ Slide to cancel'), findsOneWidget);
      await gesture.moveBy(const Offset(-120, 0));
      await tester.pump();
      expect(find.text('Release to cancel'), findsOneWidget);
      await gesture.up();
      await settle(tester);

      expect(voice.cancelled, 1);
      expect(voice.stopped, 0);
      expect(voicePreview, findsNothing);
      expect(recordingBar, findsNothing);
      expect(core.sentRecordings, isEmpty);
      expect(core.messages, isEmpty);
      expect(File(voice.lastPath!).existsSync(), isFalse);
      // Back to the message box.
      expect(composer, findsOneWidget);
    });

    // spec: app-ui/record-a-voice-message
    testWidgets('sliding a little and coming back does not cancel', (
      tester,
    ) async {
      final core = groupAsBob();
      final voice = FakeVoicePlugins();
      await openAppWithVoice(tester, core, voice);
      await record(tester, slide: 30);
      expect(voice.cancelled, 0);
      expect(voicePreview, findsOneWidget);
    });

    // spec: app-ui/record-a-voice-message
    testWidgets('a tap is not a message: too short a recording is dropped '
        'with a hint', (tester) async {
      final core = groupAsBob();
      final voice = FakeVoicePlugins()
        ..length = const Duration(milliseconds: 300);
      await openAppWithVoice(tester, core, voice);
      await record(tester, held: const Duration(milliseconds: 100));

      expect(voicePreview, findsNothing);
      expect(find.text(voiceHoldHint), findsOneWidget);
      expect(File(voice.lastPath!).existsSync(), isFalse);
      expect(core.sentRecordings, isEmpty);
    });

    // spec: app-ui/record-a-voice-message
    testWidgets('delete throws the recording away', (tester) async {
      final core = groupAsBob();
      final voice = FakeVoicePlugins();
      await openAppWithVoice(tester, core, voice);
      await record(tester);
      expect(voicePreview, findsOneWidget);

      await tap(tester, deleteVoice);
      expect(voicePreview, findsNothing);
      expect(composer, findsOneWidget);
      expect(File(voice.lastPath!).existsSync(), isFalse);
      expect(core.sentRecordings, isEmpty);
    });

    // spec: app-ui/record-a-voice-message
    testWidgets('the preview plays the recording before it is sent', (
      tester,
    ) async {
      final core = groupAsBob();
      final voice = FakeVoicePlugins();
      await openAppWithVoice(tester, core, voice);
      await record(tester);

      await tap(tester, find.byKey(const Key('preview-play')));
      expect(voice.opened, [voice.lastPath]);
      expect(voice.plays, 1);
      expect(find.byTooltip('Pause'), findsOneWidget);
      // Playing it sent nothing.
      expect(core.sentRecordings, isEmpty);
    });

    // spec: app-ui/record-a-voice-message
    testWidgets('without the microphone permission nothing is recorded', (
      tester,
    ) async {
      final core = groupAsBob();
      final voice = FakeVoicePlugins()..permission = false;
      await openAppWithVoice(tester, core, voice);
      await record(tester);

      expect(voice.started, 0);
      expect(find.text(microphoneDeniedText), findsOneWidget);
      expect(voicePreview, findsNothing);
    });

    // spec: app-ui/record-a-voice-message
    testWidgets('a clip the core refuses stays in the preview with the '
        'reason', (tester) async {
      final core = groupAsBob()
        ..fails['sendAudio'] = const CoreError(
          kind: CoreErrorKind.audioTooLong,
          message: 'too long',
        );
      final voice = FakeVoicePlugins();
      await openAppWithVoice(tester, core, voice);
      await record(tester);

      await tap(tester, sendVoice);
      expect(find.textContaining('too long to send'), findsOneWidget);
      expect(voicePreview, findsOneWidget);
      expect(File(voice.lastPath!).existsSync(), isTrue);
    });

    // spec: app-ui/record-a-voice-message
    testWidgets('the longest message stops by itself', (tester) async {
      final core = groupAsBob();
      final voice = FakeVoicePlugins()..length = voiceLimit;
      await openAppWithVoice(tester, core, voice);

      final gesture = await tester.startGesture(tester.getCenter(mic));
      await tester.pump(const Duration(milliseconds: 50));
      await tester.pump(voiceLimit + const Duration(seconds: 1));
      await settle(tester);
      expect(voice.stopped, 1);
      expect(voicePreview, findsOneWidget);
      await gesture.up();
      await settle(tester);
      expect(voice.stopped, 1, reason: 'letting go afterwards stops nothing');
    });

    // spec: app-ui/record-a-voice-message
    testWidgets('typing text turns the mic back into Send', (tester) async {
      final core = groupAsBob();
      await openAppWithVoice(tester, core, FakeVoicePlugins());
      expect(mic, findsOneWidget);
      expect(find.byTooltip('Send'), findsNothing);

      await tester.enterText(composer, 'hello');
      await tester.pump();
      expect(mic, findsNothing);
      expect(find.byTooltip('Send'), findsOneWidget);
    });
  });

  group('play a voice message', () {
    // spec: app-ui/play-a-voice-message
    testWidgets('the first tap downloads and plays, and the time follows the '
        'clip', (tester) async {
      final core = groupAsBob();
      final voice = FakeVoicePlugins();
      await openAppWithVoice(tester, core, voice);
      final message = core.receiveAudio(alice, durationMs: 7000);
      await settle(tester);

      // Nothing downloads by itself; the bubble shows the length.
      expect(core.calls, isNot(contains('downloadAudio')));
      expect(find.text('0:07'), findsOneWidget);
      expect(find.byKey(const Key('audio-waveform')), findsOneWidget);

      await tap(tester, play);
      expect(core.calls, contains('downloadAudio'));
      expect(core.args['downloadAudio'], message.id);
      expect(voice.opened, ['/media/clip-${message.id}.opus']);
      expect(voice.plays, 1);
      expect(find.byTooltip('Pause'), findsOneWidget);

      voice.players.single.seekedTo(const Duration(seconds: 3));
      await tester.pump();
      expect(find.text('0:03'), findsOneWidget);

      // Pause, then play again from where it was: no second download.
      await tap(tester, find.byTooltip('Pause'));
      expect(find.byTooltip('Play'), findsOneWidget);
      await tap(tester, play);
      expect(core.calls.where((c) => c == 'downloadAudio'), hasLength(1));
      expect(voice.plays, 2);
    });

    // spec: app-ui/play-a-voice-message
    testWidgets('starting another clip stops the first, and nothing starts by '
        'itself when one ends', (tester) async {
      final core = groupAsBob();
      final voice = FakeVoicePlugins();
      await openAppWithVoice(tester, core, voice);
      final first = core.receiveAudio(alice);
      final second = core.receiveAudio(alice);
      final third = core.receiveAudio(alice);
      await settle(tester);

      await tap(tester, play.at(0));
      expect(voice.opened, ['/media/clip-${first.id}.opus']);
      expect(find.byTooltip('Pause'), findsOneWidget);

      await tap(tester, play.at(1));
      // The first was stopped; only the second plays.
      expect(voice.stops, 1);
      expect(voice.opened.last, '/media/clip-${second.id}.opus');
      expect(find.byTooltip('Pause'), findsOneWidget);
      expect(find.byTooltip('Play'), findsNWidgets(2));

      // The second ends: the third does not begin.
      voice.players.single.finish();
      await settle(tester);
      expect(find.byTooltip('Pause'), findsNothing);
      expect(voice.opened, hasLength(2));
      expect(voice.opened, isNot(contains('/media/clip-${third.id}.opus')));
    });

    // spec: app-ui/play-a-voice-message
    testWidgets('an expired voice message says so and downloads nothing', (
      tester,
    ) async {
      final core = groupAsBob();
      await openAppWithVoice(tester, core, FakeVoicePlugins());
      core.receiveAudio(alice, expired: true);
      await settle(tester);

      expect(find.text(audioExpiredText), findsOneWidget);
      expect(play, findsNothing);
      expect(core.calls, isNot(contains('downloadAudio')));
    });

    // spec: app-ui/play-a-voice-message
    testWidgets('a clip the server turns out not to have shows as expired '
        'when tapped', (tester) async {
      final core = groupAsBob();
      final voice = FakeVoicePlugins();
      await openAppWithVoice(tester, core, voice);
      final message = core.receiveAudio(alice);
      core.serverClips.remove(message.id);
      await settle(tester);

      await tap(tester, play);
      expect(find.text(audioExpiredText), findsOneWidget);
      expect(voice.plays, 0);
    });

    // spec: app-ui/play-a-voice-message
    testWidgets('a failed download says so and can be tried again', (
      tester,
    ) async {
      final core = groupAsBob();
      final voice = FakeVoicePlugins();
      await openAppWithVoice(tester, core, voice);
      core.receiveAudio(alice);
      core.fails['downloadAudio'] = const CoreError(
        kind: CoreErrorKind.offline,
        message: 'offline',
      );
      await settle(tester);

      await tap(tester, play);
      expect(find.text(audioFailedText), findsOneWidget);

      core.fails.remove('downloadAudio');
      await tap(tester, play);
      expect(find.text(audioFailedText), findsNothing);
      expect(voice.plays, 1);
    });

    // spec: app-ui/play-a-voice-message
    testWidgets('the sender can resend a voice message from the long-press '
        'menu', (tester) async {
      final core = groupAsBob();
      await openAppWithVoice(tester, core, FakeVoicePlugins());
      final id = await core.sendAudio(Uint8List.fromList([1]));
      core.emit(CoreEvent.sendStatus(id: id, status: const SendState.accepted()));
      await settle(tester);
      // A fresh read of the history shows the sent clip.
      await tester.pump(const Duration(milliseconds: 100));

      await tester.longPress(find.byKey(Key('audio-$id')));
      await settle(tester);
      expect(find.text(resendVoiceLabel), findsOneWidget);
      await tap(tester, find.byKey(const Key('resend-voice')));
      expect(core.calls, contains('resendAudio'));
    });

    // spec: app-ui/play-a-voice-message
    testWidgets('leaving the chat stops the clip', (tester) async {
      final core = groupAsBob();
      final voice = FakeVoicePlugins();
      await openAppWithVoice(tester, core, voice);
      core.receiveAudio(alice);
      await settle(tester);
      await tap(tester, play);
      expect(voice.plays, 1);

      await tester.pageBack();
      await settle(tester);
      expect(voice.players.single.isPlaying, isFalse);
    });
  });
}
