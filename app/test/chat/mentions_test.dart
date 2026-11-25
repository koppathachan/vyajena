// app-ui: Mention picker; Mentions of me; Reply to a message (change
// `mentions`).
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/chat/chat_screen.dart';
import 'package:vyajena/chat/message_tile.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

const carol = UserRef(username: 'carol', serverId: 'chat');

/// [me]'s device in a group of akhil, alice, bob and carol, where bob is
/// "Bobby" and alice "Ali".
FakeCoreService groupAs(UserRef me) => FakeCoreService(storedUser: me)
  ..memberDevices = const [
    MemberDevice(user: akhil, device: '01'),
    MemberDevice(user: alice, device: '02'),
    MemberDevice(user: bob, device: '03'),
    MemberDevice(user: carol, device: '04'),
  ]
  ..displayNames = {
    'alice@chat': 'Ali (alice)',
    'bob@chat': 'Bobby (bob)',
    'carol@chat': 'carol',
    'akhil@chat': 'akhil',
  }
  ..profiles = {
    'alice@chat': const ProfileInfo(displayName: 'Ali'),
    'bob@chat': const ProfileInfo(displayName: 'Bobby'),
  };

final composer = find.byKey(const Key('composer'));

String composerText(WidgetTester tester) =>
    tester.widget<TextField>(composer).controller?.text ?? '';

/// The sendMessage arguments the fake recorded last.
({String text, List<MentionSpan> mentions, int? replyTo}) lastSend(
  FakeCoreService core,
) =>
    core.args['sendMessage']!
        as ({String text, List<MentionSpan> mentions, int? replyTo});

/// The rich text span drawn for [text], within the tile of [message].
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

/// Pumps enough frames for the chat's step-wise scrolling to finish.
Future<void> settleLong(WidgetTester tester) async {
  for (var i = 0; i < 60; i++) {
    await tester.pump(const Duration(milliseconds: 16));
  }
}

const mentionOfBob = MentionSpan(
  target: MentionTarget.user(user: bob),
  start: 3,
  length: 4,
);

void main() {
  // spec: app-ui/mention-picker
  testWidgets('Alice types "hi @bo" and picks "Bobby (bob)": the composer '
      'shows "hi @Bobby " and the core gets a mention of bob@chat', (
    tester,
  ) async {
    final core = groupAs(alice);
    await openApp(tester, core);
    await tester.enterText(composer, 'hi @bo');
    await settle(tester);

    final picker = find.byKey(const Key('mention-picker'));
    expect(picker, findsOneWidget);
    expect(
      find.descendant(of: picker, matching: find.text('Bobby (bob)')),
      findsOneWidget,
    );
    // Not Alice herself, nor names that do not start with "bo".
    expect(
      find.descendant(of: picker, matching: find.text('Ali')),
      findsNothing,
    );
    expect(
      find.descendant(of: picker, matching: find.text('carol')),
      findsNothing,
    );

    await tap(tester, find.text('Bobby (bob)'));
    expect(composerText(tester), 'hi @Bobby ');
    expect(picker, findsNothing);

    await tester.enterText(composer, '${composerText(tester)}look');
    await tap(tester, find.byTooltip('Send'));
    final sent = lastSend(core);
    expect(sent.text, 'hi @Bobby look');
    expect(sent.mentions, [
      const MentionSpan(
        target: MentionTarget.user(user: bob),
        start: 3,
        length: 6,
      ),
    ]);
    expect(sent.replyTo, isNull);
  });

  // spec: app-ui/mention-picker
  testWidgets('Carol types "@" and picks everyone: "@everyone " and a '
      'mention of everyone', (tester) async {
    final core = groupAs(carol);
    await openApp(tester, core);
    await tester.enterText(composer, '@');
    await settle(tester);
    // Every other member is offered, and everyone.
    final picker = find.byKey(const Key('mention-picker'));
    for (final name in ['Ali (alice)', 'Bobby (bob)', 'akhil', 'everyone']) {
      expect(
        find.descendant(of: picker, matching: find.textContaining(name)),
        findsWidgets,
        reason: name,
      );
    }
    await tap(tester, find.text('everyone'));
    expect(composerText(tester), '@everyone ');
    await tester.enterText(composer, '@everyone dinner');
    await tap(tester, find.byTooltip('Send'));
    expect(lastSend(core).mentions, [
      const MentionSpan(target: MentionTarget.everyone(), start: 0, length: 9),
    ]);
  });

  // spec: app-ui/mention-picker
  testWidgets('Name edited away: deleting a letter of "@Bobby" sends no '
      'mention', (tester) async {
    final core = groupAs(alice);
    await openApp(tester, core);
    await tester.enterText(composer, 'hi @b');
    await settle(tester);
    await tap(tester, find.text('Bobby (bob)'));
    expect(composerText(tester), 'hi @Bobby ');
    await tester.enterText(composer, 'hi @Bobb ');
    await tap(tester, find.byTooltip('Send'));
    // A plain text: no mention left.
    expect(core.calls, contains('sendText'));
    expect(core.calls, isNot(contains('sendMessage')));
    expect(core.sent.last, 'hi @Bobb ');
  });

  // spec: app-ui/mention-picker
  testWidgets('typing before a mention keeps it, moved', (tester) async {
    final core = groupAs(alice);
    await openApp(tester, core);
    await tester.enterText(composer, '@b');
    await settle(tester);
    await tap(tester, find.text('Bobby (bob)'));
    await tester.enterText(composer, 'ok @Bobby ');
    await tap(tester, find.byTooltip('Send'));
    expect(lastSend(core).mentions, [
      const MentionSpan(
        target: MentionTarget.user(user: bob),
        start: 3,
        length: 6,
      ),
    ]);
  });

  // spec: app-ui/mentions-of-me
  testWidgets('Bob is mentioned: "@Bobby" is highlighted, and the mention '
      'on screen counts as seen', (tester) async {
    final core = groupAs(bob);
    await openApp(tester, core);
    final message = core.receive(
      alice,
      'hi @Bob',
      mentions: [mentionOfBob],
      mentionsMe: true,
    );
    await settle(tester);

    // Drawn with Bob's own name for himself, highlighted as his.
    final span = spanOf(tester, '@Bobby');
    expect(span, isNotNull);
    expect(span!.style?.backgroundColor, isNotNull);
    expect(span.semanticsLabel, 'mention of you');
    // On screen in the foreground: seen, no badge left.
    expect(core.calls, contains('markMentionSeen'));
    expect(core.args['markMentionSeen'], message.id);
    expect(core.unseen, isEmpty);
    expect(find.byKey(const Key('mention-badge')), findsNothing);
  });

  // spec: app-ui/mentions-of-me
  testWidgets('Not mentioned: Carol sees "@Bobby" as a mention, not as hers, '
      'and no badge', (tester) async {
    final core = groupAs(carol);
    await openApp(tester, core);
    core.receive(alice, 'hi @Bob', mentions: [mentionOfBob]);
    await settle(tester);
    final span = spanOf(tester, '@Bobby');
    expect(span, isNotNull);
    expect(span!.style?.backgroundColor, isNull);
    expect(span.semanticsLabel, isNull);
    expect(find.byKey(const Key('mention-badge')), findsNothing);
    expect(core.calls, isNot(contains('markMentionSeen')));
  });

  // spec: app-ui/mentions-of-me
  testWidgets('@everyone is highlighted for everyone', (tester) async {
    final core = groupAs(carol);
    await openApp(tester, core);
    core.receive(
      bob,
      '@everyone dinner',
      mentions: const [
        MentionSpan(target: MentionTarget.everyone(), start: 0, length: 9),
      ],
      mentionsMe: true,
    );
    await settle(tester);
    expect(spanOf(tester, '@everyone')?.semanticsLabel, 'mention of you');
  });

  // spec: app-ui/mentions-of-me
  testWidgets('Badge: Bob, scrolled away from two mentions, sees @ 2; '
      'tapping it jumps to the older and leaves 1', (tester) async {
    final core = groupAs(bob);
    // Two mentions far up the history, then many messages after them.
    final first = core.receive(
      alice,
      'hi @Bob',
      mentions: [mentionOfBob],
      mentionsMe: true,
      emitEvent: false,
    );
    for (var i = 0; i < 25; i++) {
      core.receive(akhil, 'filler $i', emitEvent: false);
    }
    final second = core.receive(
      alice,
      'yo @Bob',
      mentions: [mentionOfBob],
      mentionsMe: true,
      emitEvent: false,
    );
    for (var i = 25; i < 50; i++) {
      core.receive(akhil, 'filler $i', emitEvent: false);
    }
    // Bob had read to the newest message, so the chat opens at the bottom,
    // scrolled past the two older mentions (which stay unseen). Without a
    // read mark every message is unread and unread-divider would open on the
    // first one, putting the mentions on screen.
    core.readMarks[0] = core.messages.last.id;
    await openApp(tester, core);
    await settle(tester);

    final badge = find.byKey(const Key('mention-badge'));
    expect(badge, findsOneWidget);
    expect(
      find.descendant(of: badge, matching: find.text('2')),
      findsOneWidget,
    );
    expect(core.unseen, [first.id, second.id]);

    await tester.tap(badge);
    await settleLong(tester);
    expect(core.unseen, [second.id]);
    expect(
      find.descendant(of: badge, matching: find.text('1')),
      findsOneWidget,
    );
    // The older mention is on screen now.
    expect(find.text('filler 0'), findsOneWidget);
  });

  // spec: app-ui/reply-to-a-message
  testWidgets('Swipe to reply: Bob swipes "dinner at 8?" right and sends '
      '"yes"; the reply shows Ali and the quote', (tester) async {
    final core = groupAs(bob);
    await openApp(tester, core);
    final original = core.receive(alice, 'dinner at 8?');
    await settle(tester);

    await tester.drag(find.text('dinner at 8?'), const Offset(300, 0));
    await settle(tester);
    final bar = find.byKey(const Key('reply-bar'));
    expect(bar, findsOneWidget);
    expect(
      find.descendant(of: bar, matching: find.text('Replying to Ali (alice)')),
      findsOneWidget,
    );
    // The original is still there (a swipe never dismisses).
    expect(find.text('dinner at 8?'), findsNWidgets(2));
    // The keyboard stays up: the swipe moves focus to the composer so the
    // reply can be typed straight away (notif-reliability-and-swipe-fixes).
    expect(tester.widget<TextField>(composer).focusNode?.hasFocus, isTrue);

    await tester.enterText(composer, 'yes');
    await tap(tester, find.byTooltip('Send'));
    expect(lastSend(core).replyTo, original.id);
    expect(bar, findsNothing);

    // The reply: "Ali (alice)" and the quote above "yes".
    final reply = find.ancestor(
      of: find.text('yes'),
      matching: find.byType(MessageTile),
    );
    expect(
      find.descendant(of: reply, matching: find.text('Ali (alice)')),
      findsOneWidget,
    );
    expect(
      find.descendant(of: reply, matching: find.text('dinner at 8?')),
      findsOneWidget,
    );
  });

  // spec: app-ui/reply-to-a-message
  testWidgets('Long-press to reply: the composer shows the quote and a '
      'cancel button', (tester) async {
    final core = groupAs(bob);
    await openApp(tester, core);
    core.receive(alice, 'dinner at 8?');
    await settle(tester);

    await tester.longPress(find.text('dinner at 8?'));
    await settle(tester);
    await tap(tester, find.text(replyLabel));
    final bar = find.byKey(const Key('reply-bar'));
    expect(bar, findsOneWidget);
    expect(
      find.descendant(of: bar, matching: find.text('dinner at 8?')),
      findsOneWidget,
    );
    await tap(tester, find.byTooltip('Cancel reply'));
    expect(bar, findsNothing);
    await tester.enterText(composer, 'plain');
    await tap(tester, find.byTooltip('Send'));
    expect(core.calls, contains('sendText'));
    expect(core.calls, isNot(contains('sendMessage')));
  });

  // spec: app-ui/reply-to-a-message
  testWidgets('Jump to the original: tapping the quote scrolls back to it', (
    tester,
  ) async {
    final core = groupAs(bob);
    final original = core.receive(alice, 'dinner at 8?', emitEvent: false);
    for (var i = 0; i < 40; i++) {
      core.receive(akhil, 'filler $i', emitEvent: false);
    }
    core.receive(
      carol,
      'me too',
      reply: ReplyQuote(
        to: alice,
        quote: 'dinner at 8?',
        kind: ReplyKind.text,
        durationMs: 0,
        thumbhash: Uint8List(0),
        originalId: original.id,
        originalDeleted: false,
      ),
      emitEvent: false,
    );
    // The chat opens at the bottom (all read), so only the reply is on screen.
    core.readMarks[0] = core.messages.last.id;
    await openApp(tester, core);
    // Only the reply's quote is on screen, not the original.
    expect(find.text('dinner at 8?'), findsOneWidget);
    expect(find.text('filler 0'), findsNothing);

    await tester.tap(find.byKey(ValueKey('quote-${core.messages.last.id}')));
    await settleLong(tester);
    final tile = find.byKey(ValueKey('message-${original.id}'));
    expect(tile, findsOneWidget);
    final view = tester.getRect(find.byType(Scaffold));
    expect(view.overlaps(tester.getRect(tile)), isTrue);
    expect(find.text(originalMissingText), findsNothing);
  });

  // spec: app-ui/reply-to-a-message
  testWidgets('Original not on this device: the app says so', (tester) async {
    final core = groupAs(bob);
    await openApp(tester, core);
    final reply = core.receive(
      carol,
      'me too',
      reply: ReplyQuote(
        to: alice,
        quote: 'dinner at 8?',
        kind: ReplyKind.text,
        durationMs: 0,
        thumbhash: Uint8List(0),
        originalDeleted: false,
      ),
    );
    await settle(tester);
    await tap(tester, find.byKey(ValueKey('quote-${reply.id}')));
    expect(find.text(originalMissingText), findsOneWidget);
  });

  // spec: app-ui/mentions-of-me
  testWidgets('a mention changes nothing about notifications: the app asks '
      'the platform and push channels for nothing when one arrives', (
    tester,
  ) async {
    final calls = <MethodCall>[];
    for (final name in ['vyajena/platform', 'vyajena/push']) {
      final channel = MethodChannel(name);
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, (
        call,
      ) async {
        calls.add(call);
        return null;
      });
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          channel,
          null,
        ),
      );
    }
    final core = groupAs(bob);
    await openApp(tester, core);
    calls.clear();
    core.receive(alice, 'hi @Bob', mentions: [mentionOfBob], mentionsMe: true);
    await settle(tester);
    expect(calls, isEmpty);
  });
}
