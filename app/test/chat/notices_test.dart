// app-ui: Notices in the chat.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/chat/time_text.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

/// 2026-11-02 09:30 local time.
final missedSince = DateTime(2026, 11, 2, 9, 30).millisecondsSinceEpoch;

bool above(WidgetTester tester, Finder first, Finder second) =>
    tester.getTopLeft(first).dy < tester.getTopLeft(second).dy;

void main() {
  // spec: app-ui/notices-in-the-chat
  testWidgets('Back from holiday: the removal notice with its time, the '
      'rejoined notice, then the new messages', (tester) async {
    final core = FakeCoreService(storedUser: bob);
    await openApp(tester, core);
    core.receive(akhil, 'before');
    await settle(tester);

    core.emit(CoreEvent.removedForInactivity(missedSinceMs: missedSince));
    core.emit(const CoreEvent.rejoined());
    core.receive(akhil, 'welcome back');
    await settle(tester);

    final context = tester.element(find.text('before'));
    final removed = find.textContaining(
      'Messages since ${timeText(context, missedSince)} were not kept',
    );
    final rejoined = find.text('This phone is back in the group.');
    expect(removed, findsOneWidget);
    expect(rejoined, findsOneWidget);
    expect(above(tester, find.text('before'), removed), isTrue);
    expect(above(tester, removed, rejoined), isTrue);
    expect(above(tester, rejoined, find.text('welcome back')), isTrue);
  });

  // spec: app-ui/notices-in-the-chat
  testWidgets('a gap notice for expired messages', (tester) async {
    final core = FakeCoreService(storedUser: bob);
    await openApp(tester, core);
    core.emit(CoreEvent.gap(newestExpiredAtMs: missedSince));
    await settle(tester);
    final context = tester.element(find.byType(Scaffold).first);
    expect(
      find.text(
        'Messages up to ${timeText(context, missedSince)} expired before '
        'this phone fetched them.',
      ),
      findsOneWidget,
    );
  });

  // spec: app-ui/notices-in-the-chat
  testWidgets('Key changed: every member sees a warning naming Bob; only '
      'the admin\'s device offers to accept it', (tester) async {
    final core = FakeCoreService(storedUser: alice)
      ..displayNames = {'bob@pi': 'Bob (bob)'};
    await openApp(tester, core);
    core.emit(const CoreEvent.identityKeyChanged(user: bob));
    await settle(tester);
    expect(find.textContaining("Bob (bob)'s key changed"), findsOneWidget);
    expect(find.text('Accept new key'), findsNothing);
  });

  // spec: app-ui/notices-in-the-chat
  testWidgets('the admin accepts a changed key, which calls '
      'accept_identity_key', (tester) async {
    final core = FakeCoreService(storedUser: akhil)
      ..admin = true
      ..displayNames = {'bob@pi': 'Bob (bob)'};
    await openApp(tester, core);
    core.emit(const CoreEvent.identityKeyChanged(user: bob));
    // The core warns again each time it meets the key: one warning.
    core.emit(const CoreEvent.identityKeyChanged(user: bob));
    await settle(tester);
    expect(find.textContaining("Bob (bob)'s key changed"), findsOneWidget);

    await tap(tester, find.text('Accept new key'));
    expect(core.calls, contains('acceptIdentityKey'));
    expect(core.args['acceptIdentityKey'], bob);
    expect(find.text("Bob (bob)'s new key was accepted."), findsOneWidget);
    expect(find.text('Accept new key'), findsNothing);
  });

  // spec: app-ui/notices-in-the-chat
  testWidgets('a refused commit and a server identity error are shown', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: bob);
    await openApp(tester, core);
    core.emit(const CoreEvent.commitRefused(reason: 'bad certificate'));
    core.emit(const CoreEvent.serverIdentityError());
    await settle(tester);
    expect(find.textContaining('bad certificate'), findsOneWidget);
    expect(
      find.textContaining("server's identity does not match"),
      findsOneWidget,
    );
  });

  // spec: app-ui/notices-in-the-chat
  testWidgets('Back from holiday, when the core catches up before the chat '
      'is on screen: the notices still come in order', (tester) async {
    final core = FakeCoreService(storedUser: bob)
      ..messages = [
        const ChatMessage(
          id: 1,
          sender: akhil,
          senderDevice: 'cd34',
          outgoing: false,
          body: ChatBody.text(text: 'before'),
          acceptedAtMs: 1,
        ),
      ];
    // The core emits its catch-up events as soon as it starts, before the
    // chat screen has drawn its first frame.
    core.onStart = () {
      core.emit(CoreEvent.removedForInactivity(missedSinceMs: missedSince));
      core.emit(const CoreEvent.rejoined());
      core.receive(akhil, 'welcome back');
    };
    await openApp(tester, core);

    final removed = find.textContaining('were not kept');
    final rejoined = find.text('This phone is back in the group.');
    expect(removed, findsOneWidget);
    expect(rejoined, findsOneWidget);
    expect(above(tester, find.text('before'), removed), isTrue);
    expect(above(tester, removed, rejoined), isTrue);
    expect(above(tester, rejoined, find.text('welcome back')), isTrue);
  });

  // spec: app-ui/notices-in-the-chat
  testWidgets('a key-changed warning that comes while the chat is closed is '
      'shown once the chat opens', (tester) async {
    // Bob waits to be added: the chat is not on screen.
    final core = FakeCoreService(storedUser: bob)
      ..epochValue = null
      ..displayNames = {'alice@pi': 'Ali (alice)'};
    await openApp(tester, core);
    expect(find.textContaining("Waiting for the admin's device"), findsOne);

    core.emit(const CoreEvent.identityKeyChanged(user: alice));
    await settle(tester);
    core.epochValue = BigInt.one;
    core.emit(const CoreEvent.joinedGroup());
    await settle(tester);

    expect(find.textContaining("Ali (alice)'s key changed"), findsOneWidget);
  });

  // spec: app-ui/notices-in-the-chat
  testWidgets('notices stay in the chat after leaving and reopening it', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: bob);
    await openApp(tester, core);
    core.receive(akhil, 'first');
    core.emit(CoreEvent.gap(newestExpiredAtMs: missedSince));
    await settle(tester);

    // Members is on top of the chat while a notice comes.
    await menu(tester, 'Members');
    core.emit(const CoreEvent.identityKeyChanged(user: alice));
    core.receive(akhil, 'second');
    await settle(tester);
    await tester.pageBack();
    await settle(tester);

    final gap = find.textContaining('expired before this phone fetched them');
    final key = find.textContaining("alice's key changed");
    expect(gap, findsOneWidget);
    expect(key, findsOneWidget);
    expect(above(tester, find.text('first'), gap), isTrue);
    expect(above(tester, gap, key), isTrue);
    expect(above(tester, key, find.text('second')), isTrue);
  });
}
