// app-ui: Send an image; Image bubble and viewer (change `media-blobs`).
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/chat/image_bubble.dart';
import 'package:vyajena/chat/message_tile.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

/// Bob's device in a group with akhil and alice, where alice is "Ali".
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

final composer = find.byKey(const Key('composer'));
final attach = find.byKey(const Key('attach-image'));
final preview = find.byKey(const Key('picture-preview'));
final bubble = find.byKey(const Key('image-bubble'));
final send = find.byTooltip('Send');

/// The bytes of a picked file (a tiny PNG, so the preview can draw it): the
/// app passes them on as they are.
final Uint8List picked = FakeCoreService.blurPng;

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

Future<void> typeCaption(WidgetTester tester, String text) async {
  await tester.enterText(composer, text);
  await tester.pump();
}

const mentionOfBob = MentionSpan(
  target: MentionTarget.user(user: bob),
  start: 0,
  length: 4,
);

void main() {
  // spec: app-ui/send-an-image
  testWidgets('Bob picks a picture, types a caption and sends: the core gets '
      'the picked bytes as they are and the caption', (tester) async {
    final core = groupAsBob();
    final picker = FakePicker(picked);
    await openAppWithPictures(tester, core, picker);

    await tap(tester, attach);
    expect(picker.opened, 1);
    expect(preview, findsOneWidget);
    // The text box now holds the caption.
    expect(find.text('Add a caption'), findsOneWidget);

    await typeCaption(tester, 'my cat');
    await tap(tester, send);

    expect(core.calls, contains('sendImage'));
    final call =
        core.args['sendImage']!
            as ({int chatId, String? caption, List<MentionSpan> mentions});
    expect(call.chatId, 0);
    expect(call.caption, 'my cat');
    // The app did no image work: the same bytes went to the core.
    expect(core.sentPictures.single, picked);
    expect(core.sent, isEmpty, reason: 'it was not sent as text');
    // The composer is ready for the next message.
    expect(preview, findsNothing);
    expect(composerText(tester), '');
    // And the message is in the chat, as a picture with its caption.
    expect(bubble, findsOneWidget);
    expect(find.text('my cat'), findsOneWidget);
  });

  // spec: app-ui/send-an-image
  testWidgets('a picture needs no caption', (tester) async {
    final core = groupAsBob();
    await openAppWithPictures(tester, core, FakePicker(picked));
    await tap(tester, attach);
    await tap(tester, send);
    final call =
        core.args['sendImage']!
            as ({int chatId, String? caption, List<MentionSpan> mentions});
    expect(call.caption, isNull);
    expect(call.mentions, isEmpty);
  });

  // spec: app-ui/send-an-image
  testWidgets('the caption has the same mention picker: its mentions are '
      'passed with the picture', (tester) async {
    final core = groupAsBob();
    await openAppWithPictures(tester, core, FakePicker(picked));
    await tap(tester, attach);
    await typeCaption(tester, '@al');
    await tap(tester, find.text('Ali (alice)'));
    await tap(tester, send);
    final call =
        core.args['sendImage']!
            as ({int chatId, String? caption, List<MentionSpan> mentions});
    expect(call.caption, '@Ali ');
    expect(call.mentions.single.target, MentionTarget.user(user: alice));
    expect(call.mentions.single.start, 0);
    expect(call.mentions.single.length, 4);
  });

  // spec: app-ui/send-an-image
  testWidgets('removing the picture sends nothing; choosing none shows '
      'nothing', (tester) async {
    final core = groupAsBob();
    final picker = FakePicker(picked);
    await openAppWithPictures(tester, core, picker);
    await tap(tester, attach);
    expect(preview, findsOneWidget);
    await tap(tester, find.byKey(const Key('remove-picture')));
    expect(preview, findsNothing);
    expect(core.calls, isNot(contains('sendImage')));

    // The gallery closed with no choice (the fake gives nothing now).
    await tap(tester, attach);
    expect(picker.opened, 2);
    expect(preview, findsNothing);
  });

  // spec: app-ui/send-an-image
  testWidgets('a picture the core refuses stays in the composer with the '
      'reason', (tester) async {
    final core = groupAsBob()
      ..fails['sendImage'] = const CoreError(
        kind: CoreErrorKind.imageUnsupported,
        message: 'x',
      );
    await openAppWithPictures(tester, core, FakePicker(picked));
    await tap(tester, attach);
    await typeCaption(tester, 'hi');
    await tap(tester, send);
    expect(
      find.text('That file is not a picture the app can send.'),
      findsOneWidget,
    );
    expect(preview, findsOneWidget, reason: 'nothing is lost');
    expect(composerText(tester), 'hi');
  });

  // spec: app-ui/image-bubble-and-viewer
  testWidgets('a received picture shows its blur at its aspect ratio; a tap '
      'downloads it and a second tap opens it full screen', (tester) async {
    final core = groupAsBob();
    await openAppWithPictures(tester, core, FakePicker());
    final message = core.receiveImage(alice, caption: 'look!');
    await settle(tester);

    expect(find.byKey(const Key('image-blur')), findsOneWidget);
    expect(find.byKey(const Key('image-picture')), findsNothing);
    final size = tester.getSize(bubble);
    expect(size.width / size.height, closeTo(640 / 480, 0.01));
    // The size to download is shown, and the caption under the picture.
    expect(find.text('1.2 MB'), findsOneWidget);
    expect(find.text('look!'), findsOneWidget);
    expect(core.calls, isNot(contains('downloadImage')));

    await tap(tester, bubble);
    expect(core.args['downloadImage'], message.id);
    expect(find.byKey(const Key('image-picture')), findsOneWidget);
    expect(find.byKey(const Key('image-blur')), findsNothing);
    expect(find.text('1.2 MB'), findsNothing);

    await tap(tester, bubble);
    expect(find.byKey(const Key('image-viewer')), findsOneWidget);
    expect(find.byType(InteractiveViewer), findsOneWidget);
    await tester.pageBack();
    await settle(tester);
    expect(find.byKey(const Key('image-viewer')), findsNothing);
  });

  // spec: app-ui/image-bubble-and-viewer
  testWidgets('a tall picture is drawn within the limits, in its own shape', (
    tester,
  ) async {
    final core = groupAsBob();
    await openAppWithPictures(tester, core, FakePicker());
    core.receiveImage(alice, width: 1500, height: 3000);
    await settle(tester);
    final size = tester.getSize(bubble);
    expect(size.width / size.height, closeTo(0.5, 0.01));
    expect(size.height, lessThanOrEqualTo(320));
  });

  // spec: app-ui/image-bubble-and-viewer
  testWidgets('a picture this device already has opens without a tap', (
    tester,
  ) async {
    final core = groupAsBob();
    await openAppWithPictures(tester, core, FakePicker(picked));
    await tap(tester, attach);
    await tap(tester, send);
    // The sender's own picture is in its cache: no blur, no download.
    expect(find.byKey(const Key('image-picture')), findsOneWidget);
    expect(find.byKey(const Key('image-download')), findsNothing);
    expect(core.calls, isNot(contains('downloadImage')));
  });

  // spec: app-ui/image-bubble-and-viewer
  testWidgets('a caption draws its mention with the reader\'s name for the '
      'user, highlighted when it names the reader', (tester) async {
    final core = groupAsBob();
    await openAppWithPictures(tester, core, FakePicker());
    core.receiveImage(alice, caption: '@Bob look', mentions: [mentionOfBob]);
    await settle(tester);
    // Bob is "Bobby" to himself; the mention names him, so it stands out.
    final mention = spanOf(tester, '@Bobby');
    expect(mention, isNotNull);
    expect(mention!.style?.fontWeight, FontWeight.bold);
    expect(spanOf(tester, ' look'), isNotNull);
  });

  // spec: app-ui/image-bubble-and-viewer
  testWidgets('an expired picture keeps its blur and says "Media expired — '
      'resend"; a tap downloads nothing', (tester) async {
    final core = groupAsBob();
    await openAppWithPictures(tester, core, FakePicker());
    core.receiveImage(alice, expired: true);
    await settle(tester);
    expect(find.text(mediaExpiredText), findsOneWidget);
    expect(find.byKey(const Key('image-blur')), findsOneWidget);
    expect(find.byKey(const Key('image-download')), findsNothing);
    await tap(tester, bubble);
    expect(core.calls, isNot(contains('downloadImage')));
  });

  // spec: app-ui/image-bubble-and-viewer
  testWidgets('a picture found expired on a tap shows as expired', (
    tester,
  ) async {
    final core = groupAsBob();
    await openAppWithPictures(tester, core, FakePicker());
    final message = core.receiveImage(alice);
    core.piPictures.remove(message.id);
    await settle(tester);
    expect(find.text('1.2 MB'), findsOneWidget);
    await tap(tester, bubble);
    expect(find.text(mediaExpiredText), findsOneWidget);
    expect(find.byKey(const Key('image-blur')), findsOneWidget);
  });

  // spec: app-ui/image-bubble-and-viewer
  testWidgets('a download that fails offers a retry that works', (
    tester,
  ) async {
    final core = groupAsBob();
    await openAppWithPictures(tester, core, FakePicker());
    core.receiveImage(alice);
    await settle(tester);
    core.fails['downloadImage'] = const CoreError(
      kind: CoreErrorKind.offline,
      message: 'x',
    );
    await tap(tester, bubble);
    expect(find.text(imageFailedText), findsOneWidget);
    expect(find.byKey(const Key('image-blur')), findsOneWidget);

    core.fails.remove('downloadImage');
    await tap(tester, bubble);
    expect(find.byKey(const Key('image-picture')), findsOneWidget);
    expect(find.text(imageFailedText), findsNothing);
  });

  // spec: app-ui/image-bubble-and-viewer
  testWidgets('long-pressing an own picture offers to send it again', (
    tester,
  ) async {
    final core = groupAsBob();
    await openAppWithPictures(tester, core, FakePicker(picked));
    await tap(tester, attach);
    await tap(tester, send);
    await tester.longPress(bubble);
    await settle(tester);
    expect(find.text(resendPictureLabel), findsOneWidget);
    await tap(tester, find.text(resendPictureLabel));
    expect(core.calls, contains('resendImage'));
    expect(core.messages.length, 2);
  });

  // spec: app-ui/image-bubble-and-viewer
  testWidgets('another user\'s picture has no resend entry', (tester) async {
    final core = groupAsBob();
    await openAppWithPictures(tester, core, FakePicker());
    core.receiveImage(alice);
    await settle(tester);
    await tester.longPress(bubble);
    await settle(tester);
    expect(find.text(resendPictureLabel), findsNothing);
    expect(find.text(replyLabel), findsOneWidget);
  });

  // spec: app-ui/image-bubble-and-viewer
  testWidgets('replying to a picture quotes its caption', (tester) async {
    final core = groupAsBob();
    await openAppWithPictures(tester, core, FakePicker());
    core.receiveImage(alice, caption: 'the view');
    await settle(tester);
    await tester.longPress(bubble);
    await settle(tester);
    await tap(tester, find.text(replyLabel));
    expect(find.byKey(const Key('reply-bar')), findsOneWidget);
    expect(find.text('the view'), findsWidgets);
    // A picture goes as its own message: no attach button while replying.
    expect(attach, findsNothing);
  });
}

String composerText(WidgetTester tester) =>
    tester.widget<TextField>(composer).controller?.text ?? '';
