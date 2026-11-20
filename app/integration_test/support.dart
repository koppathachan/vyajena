// Shared helpers of the app's integration tests (run by run.sh): real-time
// waiting, taps, a fresh phone, joining from an invite.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/app/app.dart';
import 'package:vyajena/chat/picture_picker.dart';
import 'package:vyajena/core/rust_core_service.dart';
import 'package:vyajena/src/rust/frb_generated.dart';

/// The invite text the admin's device made for `bob`.
const invite = String.fromEnvironment('VYAJENA_INVITE');

/// A 48x32 PNG gradient: the picture the app "picks" from the gallery.
final Uint8List testPicture = base64Decode(
  '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'
  '6D8UM4gZxkgxo5hxjBIziZnGaDGzmHmMEbOIWcZYMasYYpyYdcwmxovZxuxigph9zCEmij'
  'nGnGKSmHPMJSaLucbcYoqYe8wjpop5xrximph3zCemi/nG/OL/UMIgYZggJYwSxglKwiRh'
  'mqAlzBLmCUbCImGZYCWsEkhwEtYJmwQvYZuwSwgS9gmHhCjhmHBKSBLOCZeELOGacEsoEu'
  '4Jj4Qq4ZnwSmgS3gmfhC7hm/BL/kMpg5RhipQyShmnKCmTlGmKljJLmacYKYuUZYqVskoh'
  'xUlZp2xSvJRtyi4lSNmnHFKilGPKKSVJOadcUrKUa8otpUi5pzxSqpRnyiulSXmnfFK6lG'
  '/KL/0PZQwyhhlSxihjnKFkTDKmGVrGLGOeYWQsMpYZVsYqgwwnY52xyfAythm7jCBjn3HI'
  'iDKOGaeMJOOcccnIMq4Zt4wi457xyKgynhmvjCbjnfHJ6DK+Gb/sP5QzyBnmSDmjnHGOkj'
  'PJmeZoObOceY6Rs8hZ5lg5qxxynJx1zibHy9nm7HKCnH3OISfKOeaccpKcc84lJ8u55txy'
  'ipx7ziOnynnmvHKanHfOJ6fL+eb88v9QwaBgWCAVjArGBUrBpGBaoBXMCuYFRsGiYFlgFa'
  'wKKHAK1gWbAq9gW7ArCAr2BYeCqOBYcCpICs4Fl4Ks4FpwKygK7gWPgqrgWfAqaAreBZ+C'
  'ruBb8Cv+QyWDkmGJVDIqGZcoJZOSaYlWMiuZlxgli5JliVWyKqHEKVmXbEq8km3JriQo2Z'
  'ccSqKSY8mpJCk5l1xKspJrya2kKLmXPEqqkmfJq6QpeZd8SrqSb8mv/A9VDCqGFVLFqGJc'
  'oVRMKqYVWsWsYl5hVCwqlhVWxaqCCqdiXbGp8Cq2FbuKoGJfcaiIKo4Vp4qk4lxxqcgqrh'
  'W3iqLiXvGoqCqeFa+KpuJd8anoKr4Vv+o/VDOoGdZINaOacY1SM6mZ1mg1s5p5jVGzqFnW'
  'WDWrGmqcmnXNpsar2dbsaoKafc2hJqo51pxqkppzzaUmq7nW3GqKmnvNo6aqeda8apqad8'
  '2npqv51vzq/1DDoGHYIDWMGsYNSsOkYdqgNcwa5g1Gw6Jh2WA1rBpocBrWDZsGr2HbsGsI'
  'GvYNh4ao4dhwakgazg2Xhqzh2nBrKBruDY+GquHZ8GpoGt4Nn4au4dvwa/5DLYOWYYvUMm'
  'oZtygtk5Zpi9Yya5m3GC2LlmWL1bJqocVpWbdsWryWbcuuJWjZtxxaopZjy6klaTm3XFqy'
  'lmvLraVoubc8WqqWZ8urpWl5t3xaupZvy6/9D3UMOoYdUseoY9yhdEw6ph1ax6xj3mF0LD'
  'qWHVbHqoMOp2PdsenwOrYdu46gY99x6Ig6jh2njqTj3HHpyDquHbeOouPe8eioOp4dr46m'
  '493x6eg6vh2/7j/UM+gZ9kg9o55xj9Iz6Zn2aD2znnmP0bPoWfZYPaseepyedc+mx+vZ9u'
  'x6gp59z6En6jn2nHqSnnPPpSfrufbceoqee8+jp+p59rx6mp53z6en6/n2/Hr+ADl96Nq7'
  'se9aAAAAAElFTkSuQmCC',
);

/// A picker that always returns [testPicture], so the test needs no gallery.
class TestPicker extends PicturePicker {
  const TestPicker();

  @override
  Future<Uint8List?> pick() async => testPicture;
}

/// Pumps until [finder] finds something, in real time.
Future<void> waitFor(
  WidgetTester tester,
  Finder finder, {
  Duration timeout = const Duration(seconds: 90),
}) async {
  final end = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(end)) {
    await tester.pump(const Duration(milliseconds: 50));
    if (finder.evaluate().isNotEmpty) return;
    await Future<void>.delayed(const Duration(milliseconds: 100));
  }
  fail('timed out waiting for $finder');
}

/// Pumps until [finder] finds exactly [count] widgets, in real time.
Future<void> waitForCount(
  WidgetTester tester,
  Finder finder,
  int count, {
  Duration timeout = const Duration(seconds: 90),
}) async {
  final end = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(end)) {
    await tester.pump(const Duration(milliseconds: 50));
    if (finder.evaluate().length == count) return;
    await Future<void>.delayed(const Duration(milliseconds: 100));
  }
  fail('timed out waiting for $count x $finder');
}

/// A long press that really waits: on a device `tester.longPress` does not
/// let real time pass, so the press timer would never fire.
Future<void> longPress(WidgetTester tester, Finder finder) async {
  // Earlier pictures scroll out of view (and are not built) as the chat
  // grows: scroll the message list, one way then the other, until it is.
  final list = find.byType(ListView).first;
  for (var i = 0; finder.evaluate().isEmpty && i < 40; i++) {
    await tester.drag(list, Offset(0, i < 20 ? 300 : -300));
    await tester.pump(const Duration(milliseconds: 100));
  }
  await waitFor(tester, finder);
  await tester.ensureVisible(finder);
  await tester.pump(const Duration(milliseconds: 300));
  final gesture = await tester.startGesture(tester.getCenter(finder));
  await Future<void>.delayed(const Duration(milliseconds: 900));
  await tester.pump(const Duration(milliseconds: 100));
  await gesture.up();
  await tester.pump(const Duration(milliseconds: 300));
}

Future<void> tap(WidgetTester tester, Finder finder) async {
  await waitFor(tester, finder);
  await tester.tap(finder);
  await tester.pump(const Duration(milliseconds: 100));
}

/// The status mark of the own message with [text] ("⏳" or "✓").
Finder statusOf(String text, String mark) => find.descendant(
  of: find.ancestor(of: find.text(text), matching: find.byType(Card)),
  matching: find.text(mark),
);

/// Starts the app as a fresh phone (no data from an earlier run), joins from
/// [invite] and waits for the chat, added by the admin's device on its own.
Future<RustCoreService> joinFresh(WidgetTester tester) async {
  expect(invite, isNotEmpty, reason: 'run integration_test/run.sh');
  final dataDir = await platformChannel.invokeMethod<String>('dataDir');
  for (final entry in Directory(dataDir!).listSync()) {
    entry.deleteSync(recursive: true);
  }
  await RustLib.init();
  final core = RustCoreService();
  await tester.pumpWidget(VyajenaApp(core: core, pictures: const TestPicker()));
  await core.open();
  await waitFor(tester, find.text('Join the group'));
  await tester.enterText(find.byKey(const Key('invite')), invite);
  await tap(tester, find.text('Check invite'));
  await waitFor(tester, find.text('bob@chat'));
  await tap(tester, find.text('Join'));
  // Added by the admin's device on its own: the chat list shows the group.
  await tap(tester, find.text('Group'));
  await waitFor(tester, find.byKey(const Key('composer')));
  return core;
}

/// Attaches [testPicture], types [caption] and sends.
Future<void> sendPicture(WidgetTester tester, String caption) async {
  await tap(tester, find.byKey(const Key('attach-image')));
  await waitFor(tester, find.byKey(const Key('picture-preview')));
  await tester.enterText(find.byKey(const Key('composer')), caption);
  await tap(tester, find.byTooltip('Send'));
}
