// app-ui: Notices in the chat.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/app/connection_indicator.dart';
import 'package:vyajena/chat/notice_tile.dart';
import 'package:vyajena/chat/time_text.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

/// 2026-11-02 09:30 local time.
final missedSince = DateTime(2026, 11, 2, 9, 30).millisecondsSinceEpoch;

bool above(WidgetTester tester, Finder first, Finder second) =>
    tester.getTopLeft(first).dy < tester.getTopLeft(second).dy;

const carol = UserRef(username: 'carol', serverId: 'chat');

MemberDevice dev(UserRef user, String device) =>
    MemberDevice(user: user, device: device);

/// Akhil's phone and Bob's phone are in the group.
List<MemberDevice> group() => [dev(akhil, 'a1'), dev(bob, 'b1')];

/// Every notice text in the chat, top to bottom.
List<String> noticeTexts(WidgetTester tester) => [
  for (final tile in tester.widgetList<NoticeTile>(find.byType(NoticeTile)))
    tile.text,
].reversed.toList();

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
    core.emit(CoreEvent.gap(newestExpiredAtMs: missedSince, chatId: 0));
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

  // spec: app-ui/notices-in-the-chat (safety-number-changed-prompt)
  testWidgets('Safety number changed: every member — not just the admin — '
      'sees a re-trust prompt naming the user and can re-trust it', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: alice)
      ..displayNames = {'bob@chat': 'Bob (bob)'};
    await openApp(tester, core);
    core.emit(const CoreEvent.identityKeyChanged(user: bob));
    await settle(tester);
    // A warning naming the user, phrased as a re-trust question, with an action
    // this non-admin member can take.
    expect(
      find.textContaining("Bob (bob)'s safety number changed"),
      findsOneWidget,
    );
    expect(find.text('Re-trust'), findsOneWidget);
    // Nothing is presented as verified yet: the re-trust prompt is still open.
    expect(
      find.textContaining('You re-trusted'),
      findsNothing,
    );
  });

  // spec: app-ui/notices-in-the-chat (safety-number-changed-prompt)
  testWidgets('re-trusting a changed safety number calls '
      'acknowledge_identity_key and never auto-accepts silently', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: alice)
      ..displayNames = {'akhil@chat': 'Akhil'};
    await openApp(tester, core);
    // The recovered admin re-keyed; the member sees one prompt even if the core
    // meets the key more than once.
    core.emit(const CoreEvent.identityKeyChanged(user: akhil));
    core.emit(const CoreEvent.identityKeyChanged(user: akhil));
    await settle(tester);
    expect(
      find.textContaining("Akhil's safety number changed"),
      findsOneWidget,
    );
    // Trust is withheld until the owner acts: no acknowledge happened on its own.
    expect(core.calls, isNot(contains('acknowledgeIdentityKey')));

    await tap(tester, find.text('Re-trust'));
    expect(core.calls, contains('acknowledgeIdentityKey'));
    expect(core.args['acknowledgeIdentityKey'], akhil);
    expect(
      find.text("You re-trusted Akhil's new safety number."),
      findsOneWidget,
    );
    expect(find.text('Re-trust'), findsNothing);
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
          chatId: 0,
          sender: akhil,
          senderDevice: 'cd34',
          outgoing: false,
          ownUser: false,
          body: ChatBody.text(text: 'before'),
          mentions: [],
          mentionsMe: false,
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
      ..displayNames = {'alice@chat': 'Ali (alice)'};
    await openApp(tester, core);
    expect(find.textContaining("Waiting for the admin's device"), findsOne);

    core.emit(const CoreEvent.identityKeyChanged(user: alice));
    await settle(tester);
    core.epochValue = BigInt.one;
    core.emit(const CoreEvent.joinedGroup());
    await settle(tester);
    await enterGroup(tester);

    expect(
      find.textContaining("Ali (alice)'s safety number changed"),
      findsOneWidget,
    );
  });

  // spec: app-ui/notices-in-the-chat
  testWidgets('notices stay in the chat after leaving and reopening it', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: bob);
    await openApp(tester, core);
    core.receive(akhil, 'first');
    core.emit(CoreEvent.gap(newestExpiredAtMs: missedSince, chatId: 0));
    await settle(tester);

    // Members is on top of the chat while a notice comes.
    await menu(tester, 'Members');
    core.emit(const CoreEvent.identityKeyChanged(user: alice));
    core.receive(akhil, 'second');
    await settle(tester);
    await tester.pageBack();
    await settle(tester);

    final gap = find.textContaining('expired before this phone fetched them');
    final key = find.textContaining("alice's safety number changed");
    expect(gap, findsOneWidget);
    expect(key, findsOneWidget);
    expect(above(tester, find.text('first'), gap), isTrue);
    expect(above(tester, gap, key), isTrue);
    expect(above(tester, key, find.text('second')), isTrue);
  });

  // spec: app-ui/notices-in-the-chat
  testWidgets('Bob is removed: the chat says so, the indicator shows '
      'removed, and the composer is disabled', (tester) async {
    final core = FakeCoreService(storedUser: bob);
    await openApp(tester, core);
    core.receive(akhil, 'bye');
    core.emit(const CoreEvent.connection(status: ConnectionStatus.connected));
    await settle(tester);

    core.emit(const CoreEvent.removed());
    core.emit(
      const CoreEvent.connection(status: ConnectionStatus.disconnected),
    );
    await settle(tester);
    expect(find.text(deviceRemovedText), findsOneWidget);
    expect(find.text('This device was removed from the group.'), findsOne);
    expect(above(tester, find.text('bye'), find.text(deviceRemovedText)), true);
    expect(find.byTooltip(ConnectionIndicator.removedLabel), findsOneWidget);
    expect(find.byIcon(ConnectionIndicator.removedIcon), findsOneWidget);
    final box = tester.widget<TextField>(find.byKey(const Key('composer')));
    expect(box.enabled, isFalse);
    expect(
      tester
          .widget<IconButton>(find.widgetWithIcon(IconButton, Icons.send))
          .onPressed,
      isNull,
    );
    // History stays readable.
    expect(find.text('bye'), findsOneWidget);

    // The core says it again after a restart (each start): one notice.
    await core.stop();
    await core.start();
    core.emit(const CoreEvent.removed());
    await settle(tester);
    expect(find.text(deviceRemovedText), findsOneWidget);
    expect(find.byTooltip(ConnectionIndicator.removedLabel), findsOneWidget);
  });

  // spec: app-ui/notices-in-the-chat
  testWidgets('Membership changes: "carol joined", "bob added a device" and '
      '"akhil removed carol", in that order', (tester) async {
    final core = FakeCoreService(storedUser: bob)
      ..displayNames = {'akhil@chat': 'Akhil (akhil)'}
      ..memberDevices = group();
    await openApp(tester, core);
    core.receive(akhil, 'first');
    await settle(tester);

    core.emit(
      CoreEvent.membersChanged(
        added: [dev(carol, 'c1')],
        removed: const [],
        by: akhil,
        automatic: const [],
      ),
    );
    core.emit(
      CoreEvent.membersChanged(
        added: [dev(bob, 'b2')],
        removed: const [],
        by: bob,
        automatic: const [],
      ),
    );
    core.emit(
      CoreEvent.membersChanged(
        added: const [],
        removed: [dev(carol, 'c1')],
        by: akhil,
        automatic: const [],
      ),
    );
    await settle(tester);
    expect(noticeTexts(tester), [
      'carol joined',
      'bob added a device',
      'Akhil (akhil) removed carol',
    ]);
    expect(above(tester, find.text('first'), find.text('carol joined')), true);
  });

  // spec: app-ui/notices-in-the-chat
  testWidgets('a user removing one of their devices: "bob removed a '
      'device"; all of a user\'s devices in one commit: one notice', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: akhil)
      ..admin = true
      ..memberDevices = [...group(), dev(bob, 'b2')];
    await openApp(tester, core);
    core.emit(
      CoreEvent.membersChanged(
        added: const [],
        removed: [dev(bob, 'b2')],
        by: bob,
        automatic: const [],
      ),
    );
    // Carol joins with two devices at once, then the admin removes her.
    core.emit(
      CoreEvent.membersChanged(
        added: [dev(carol, 'c1'), dev(carol, 'c2')],
        removed: const [],
        by: akhil,
        automatic: const [],
      ),
    );
    core.emit(
      CoreEvent.membersChanged(
        added: const [],
        removed: [dev(carol, 'c1'), dev(carol, 'c2')],
        by: akhil,
        automatic: const [],
      ),
    );
    await settle(tester);
    expect(noticeTexts(tester), [
      'bob removed a device',
      'carol joined',
      'akhil removed carol',
    ]);
  });

  // spec: app-ui/notices-in-the-chat
  testWidgets('Quiet inactivity re-add: no membership notice for an '
      'inactivity removal or the re-add of the same device', (tester) async {
    final core = FakeCoreService(storedUser: alice)
      ..memberDevices = [...group(), dev(alice, 'l1')];
    await openApp(tester, core);
    // The server's inactivity Remove of Bob's phone, committed by the admin.
    core.emit(
      CoreEvent.membersChanged(
        added: const [],
        removed: [dev(bob, 'b1')],
        by: akhil,
        automatic: [dev(bob, 'b1')],
      ),
    );
    // Later the admin's phone re-adds it.
    core.emit(
      CoreEvent.membersChanged(
        added: [dev(bob, 'b1')],
        removed: const [],
        by: akhil,
        automatic: [dev(bob, 'b1')],
      ),
    );
    await settle(tester);
    expect(find.byType(NoticeTile), findsNothing);

    // And Bob, back, is still known: his new tablet is "added a device".
    core.emit(
      CoreEvent.membersChanged(
        added: [dev(bob, 'b2')],
        removed: const [],
        by: bob,
        automatic: const [],
      ),
    );
    await settle(tester);
    expect(noticeTexts(tester), ['bob added a device']);
  });

  // spec: app-ui/notices-in-the-chat
  testWidgets('no membership notice for the commit that added this device', (
    tester,
  ) async {
    // Carol waits to be added.
    final core = FakeCoreService(storedUser: carol)..epochValue = null;
    await openApp(tester, core);
    core
      ..epochValue = BigInt.one
      ..memberDevices = [...group(), dev(carol, 'c1')];
    core.emit(const CoreEvent.joinedGroup());
    core.emit(
      const CoreEvent.membersChanged(added: [], removed: [], automatic: []),
    );
    await settle(tester);
    await enterGroup(tester);
    expect(find.byKey(const Key('composer')), findsOneWidget);
    expect(find.byType(NoticeTile), findsNothing);

    // Then Bob links his tablet: Carol learned the members on joining.
    core.emit(
      CoreEvent.membersChanged(
        added: [dev(bob, 'b2')],
        removed: const [],
        by: bob,
        automatic: const [],
      ),
    );
    await settle(tester);
    expect(noticeTexts(tester), ['bob added a device']);
  });
}
