// Pictures on a simulator or emulator against a real temp Pi (change
// `media-blobs`). Run it with `integration_test/run.sh <device> images`: the
// script starts the Pi (with a picture store) and the admin's device on the
// host (`bridge/tests/app_peer_images.rs`). This app is Bob's phone, and the
// picker is a fake that returns a fixed PNG, so no gallery is needed.
//
// End to end, through the real core, relay path and Pi:
//   1. Bob sends a picture with the caption "cat" (sending, then accepted);
//   2. the admin's picture arrives as a blur, a tap downloads it, a tap on the
//      loaded picture opens the viewer;
//   3. Bob sends "queued" with the connection off: it stays sending, goes out
//      (upload first, then the message) when the core runs again;
//   4. "Resend picture" in the long-press menu sends "cat" again.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  // spec: app-ui/send-an-image
  // spec: app-ui/image-bubble-and-viewer
  // spec: client-runtime/blob-crypto-and-send-order
  testWidgets('pictures: send, receive and open, queue offline, resend', (
    tester,
  ) async {
    final core = await joinFresh(tester);

    // 1. Send a picture with a caption.
    await sendPicture(tester, 'cat');
    await waitFor(tester, statusOf('cat', '✓'));
    expect(
      find.descendant(
        of: find.ancestor(of: find.text('cat'), matching: find.byType(Card)),
        matching: find.byKey(const Key('image-bubble')),
      ),
      findsOneWidget,
    );

    // 2. The admin's picture: blur first, then download on tap, then viewer.
    await waitFor(tester, find.text('from admin'));
    final theirs = find.ancestor(
      of: find.text('from admin'),
      matching: find.byType(Card),
    );
    final blurOrPicture = find.descendant(
      of: theirs,
      matching: find.byKey(const Key('image-bubble')),
    );
    expect(blurOrPicture, findsOneWidget);
    await tester.tap(blurOrPicture);
    await waitFor(
      tester,
      find.descendant(
        of: theirs,
        matching: find.byKey(const Key('image-picture')),
      ),
    );
    await tester.tap(blurOrPicture);
    await waitFor(tester, find.byKey(const Key('image-viewer')));
    await tester.tap(
      find.descendant(
        of: find.byKey(const Key('image-viewer')),
        matching: find.byTooltip('Back'),
      ),
    );
    await tester.pumpAndSettle();

    // 3. Offline: stays sending, then goes out when the core runs again.
    await core.stop();
    await sendPicture(tester, 'queued');
    await waitFor(tester, statusOf('queued', '⏳'));
    await core.start();
    await waitFor(tester, statusOf('queued', '✓'));

    // 4. Resend the first picture from the long-press menu.
    await longPress(
      tester,
      find.descendant(
        of: find.ancestor(of: find.text('cat'), matching: find.byType(Card)),
        matching: find.byKey(const Key('image-bubble')),
      ),
    );
    await tester.pumpAndSettle();
    await tap(tester, find.text('Resend picture'));
    // The new message is at the bottom, which may be a scroll away on a small
    // screen; the host peer checks that the second "cat" arrived.
    for (var i = 0; i < 6; i++) {
      await tester.drag(find.byType(ListView).first, const Offset(0, -400));
      await tester.pump(const Duration(milliseconds: 100));
    }
    await waitFor(tester, statusOf('cat', '✓'));
    await Future<void>.delayed(const Duration(seconds: 3));

    await core.stop();
  });
}
