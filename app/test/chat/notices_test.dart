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
}
