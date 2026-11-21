// app-ui: Links in messages; message-content: Text only (change `links`).
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:plugin_platform_interface/plugin_platform_interface.dart';
import 'package:url_launcher_platform_interface/link.dart';
import 'package:url_launcher_platform_interface/url_launcher_platform_interface.dart';
import 'package:vyajena/chat/links.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import 'mentions_test.dart' show groupAs, carol, spanOf;

/// A launcher that records what it was asked to open.
class FakeLauncher extends UrlLauncherPlatform with MockPlatformInterfaceMixin {
  FakeLauncher({this.canHandle = true});

  final bool canHandle;
  final List<({String url, LaunchOptions options})> launched = [];

  @override
  LinkDelegate? get linkDelegate => null;

  @override
  Future<bool> launchUrl(String url, LaunchOptions options) async {
    launched.add((url: url, options: options));
    return canHandle;
  }
}

List<String> found(String text) => [
  for (final l in findLinks(text)) text.substring(l.start, l.end),
];

/// Taps the first occurrence of [text] among the drawn rich texts.
Future<void> tapText(WidgetTester tester, String text) async {
  await tester.tapOnText(find.textRange.ofSubstring(text));
  await settle(tester);
}

void main() {
  late FakeLauncher launcher;
  late UrlLauncherPlatform original;

  setUp(() {
    original = UrlLauncherPlatform.instance;
    launcher = FakeLauncher();
    UrlLauncherPlatform.instance = launcher;
  });
  tearDown(() => UrlLauncherPlatform.instance = original);

  // spec: app-ui/links-in-messages
  test('only http and https URLs are found, with trailing punctuation '
      'trimmed', () {
    expect(found('see https://example.com/x'), ['https://example.com/x']);
    expect(found('HTTP://A.com and http://b.org/p?q=1&r=2#f'), [
      'HTTP://A.com',
      'http://b.org/p?q=1&r=2#f',
    ]);
    expect(found('go to https://a.com.'), ['https://a.com']);
    expect(found('(https://a.com/x), "https://b.com"!?'), [
      'https://a.com/x',
      'https://b.com',
    ]);
    expect(found('https://en.wikipedia.org/wiki/A_(b)'), [
      'https://en.wikipedia.org/wiki/A_(b',
    ]);
    // Not links.
    expect(found('visit example.com or www.x.io or a@b.com'), isEmpty);
    expect(found('mailto:a@b.com tel:+4912345 ftp://x.org'), isEmpty);
    expect(found('https:// and http://.'), isEmpty);
  });

  // spec: app-ui/links-in-messages
  test('ranges are UTF-16 offsets', () {
    const text = '😀 https://a.com';
    final link = findLinks(text).single;
    expect(link.start, 3);
    expect(text.substring(link.start, link.end), 'https://a.com');
  });

  // spec: app-ui/links-in-messages
  testWidgets('Bob taps the URL in Alice\'s message: a sheet shows it with '
      'Open, Copy and Cancel, and Open launches it externally', (tester) async {
    final core = groupAs(bob);
    await openApp(tester, core);
    core.receive(alice, 'see https://example.com/x');
    await settle(tester);

    await tapText(tester, 'https://example.com/x');
    expect(
      tester.widget<Text>(find.byKey(const Key('link-url'))).data,
      'https://example.com/x',
    );
    expect(find.byKey(const Key('link-open')), findsOneWidget);
    expect(find.byKey(const Key('link-copy')), findsOneWidget);
    expect(find.byKey(const Key('link-cancel')), findsOneWidget);
    // Nothing opens before Open.
    expect(launcher.launched, isEmpty);

    await tap(tester, find.byKey(const Key('link-open')));
    expect(launcher.launched.single.url, 'https://example.com/x');
    expect(
      launcher.launched.single.options.mode,
      PreferredLaunchMode.externalApplication,
    );
    expect(find.byKey(const Key('link-url')), findsNothing);
  });

  // spec: app-ui/links-in-messages
  testWidgets('Cancel and Copy open nothing; Copy puts the URL on the '
      'clipboard', (tester) async {
    String? clip;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          clip = (call.arguments as Map)['text'] as String;
        }
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );
    final core = groupAs(bob);
    await openApp(tester, core);
    core.receive(alice, 'https://example.com/x');
    await settle(tester);

    await tapText(tester, 'https://example.com/x');
    await tap(tester, find.byKey(const Key('link-cancel')));
    expect(find.byKey(const Key('link-url')), findsNothing);

    await tapText(tester, 'https://example.com/x');
    await tap(tester, find.byKey(const Key('link-copy')));
    expect(clip, 'https://example.com/x');
    expect(find.byKey(const Key('link-url')), findsNothing);
    expect(launcher.launched, isEmpty);
  });

  // spec: app-ui/links-in-messages
  testWidgets('No handler for the link: the sheet says "Couldn\'t open this '
      'link" and still offers Copy', (tester) async {
    UrlLauncherPlatform.instance = FakeLauncher(canHandle: false);
    final core = groupAs(bob);
    await openApp(tester, core);
    core.receive(alice, 'https://example.com/x');
    await settle(tester);

    await tapText(tester, 'https://example.com/x');
    await tap(tester, find.byKey(const Key('link-open')));
    expect(find.byKey(const Key('link-failed')), findsOneWidget);
    expect(find.text('Couldn\'t open this link'), findsOneWidget);
    expect(find.byKey(const Key('link-copy')), findsOneWidget);
    expect(find.byKey(const Key('link-url')), findsOneWidget);
  });

  // spec: app-ui/links-in-messages
  testWidgets('Only http/https: "example.com" and "a@b.com" are plain text, '
      'nothing is fetched', (tester) async {
    final core = groupAs(bob);
    await openApp(tester, core);
    core.receive(alice, 'visit example.com or mail a@b.com');
    await settle(tester);

    expect(find.text('visit example.com or mail a@b.com'), findsOneWidget);
    expect(spanOf(tester, 'example.com'), isNull);
    expect(launcher.launched, isEmpty);
  });

  // spec: app-ui/links-in-messages
  testWidgets('An image captioned with a URL shows it as a tappable link', (
    tester,
  ) async {
    final core = groupAs(bob);
    await openApp(tester, core);
    core.receiveImage(alice, caption: 'https://example.com/photo');
    await settle(tester);

    await tapText(tester, 'https://example.com/photo');
    expect(
      tester.widget<Text>(find.byKey(const Key('link-url'))).data,
      'https://example.com/photo',
    );
  });

  // spec: app-ui/links-in-messages
  testWidgets('A message with a mention and a URL draws both; a URL that '
      'overlaps the mention is not a link', (tester) async {
    final core = groupAs(carol);
    await openApp(tester, core);
    // "@Bobby" mention at 3..7 (4 units of the stored text "hi @Bob").
    core.receive(
      alice,
      'hi @Bob https://example.com',
      mentions: const [
        MentionSpan(target: MentionTarget.user(user: bob), start: 3, length: 4),
      ],
    );
    // A URL whose text is the mention's own characters.
    core.receive(
      alice,
      'x https://a.com/@Bob',
      mentions: const [
        MentionSpan(
          target: MentionTarget.user(user: bob),
          start: 16,
          length: 4,
        ),
      ],
    );
    await settle(tester);

    expect(spanOf(tester, '@Bobby'), isNotNull);
    expect(spanOf(tester, 'https://example.com')?.recognizer, isNotNull);
    // The second: the link range overlaps the mention, so only the mention
    // is drawn and no span of the URL is tappable.
    expect(spanOf(tester, 'https://a.com/@Bob'), isNull);
    expect(spanOf(tester, 'x https://a.com/'), isNotNull);
    expect(spanOf(tester, 'x https://a.com/')?.recognizer, isNull);
  });

  // spec: message-content/text-only
  testWidgets('A URL in a message shows no preview and opens nothing until '
      'the user taps Open', (tester) async {
    final core = groupAs(bob);
    await openApp(tester, core);
    core.receive(alice, 'https://example.com');
    await settle(tester);

    expect(find.byType(Image), findsNothing);
    expect(find.byType(RichText), findsWidgets);
    expect(launcher.launched, isEmpty);
  });
}
