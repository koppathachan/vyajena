// manual-presence: the Profile presence toggle and the avatar presence dots
// (change `manual-presence`).
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/core/core_service.dart';

import '../chat/dms_test.dart' show aliceInGroup, dmWithBob;
import '../support/app.dart';

const _green = Color(0xFF34C759);

final profileTab = find.byKey(const Key('tab-profile'));

/// A tall screen, so the whole Profile tab (including the presence toggle) is
/// built and on screen.
void tall(WidgetTester tester) {
  tester.view.physicalSize = const Size(800, 2400);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
}

/// The decoration colour of the one presence dot matched by [finder].
Color dotColour(WidgetTester tester, Finder finder) =>
    (tester.widget<Container>(finder).decoration! as ShapeDecoration).color!;

void main() {
  // spec: presence/toggle-your-status-on-the-profile-screen
  testWidgets('the Profile toggle sets Online then Offline, telling the core '
      'and recolouring the profile-tab avatar dot', (tester) async {
    tall(tester);
    final core = aliceInGroup();
    await openHome(tester, core);
    final scheme = Theme.of(
      tester.element(find.byKey(const Key('bottom-nav'))),
    ).colorScheme;
    final dot = find.descendant(
      of: profileTab,
      matching: find.byKey(const Key('profile-presence-dot')),
    );

    // Offline is the default, so the dot is the error colour to start.
    expect(dotColour(tester, dot), scheme.error);

    // Flip the Profile toggle on: the core is told Online and the dot turns
    // green.
    await tap(tester, profileTab);
    final toggle = find.byKey(const Key('presence-toggle'));
    expect(toggle, findsOneWidget);
    await tap(tester, toggle);
    await settle(tester);
    expect(core.calls, contains('setPresence'));
    expect(core.args['setPresence'], true);
    expect(dotColour(tester, dot), _green);

    // Flip it off again: Offline, and the dot is the error colour.
    await tap(tester, toggle);
    await settle(tester);
    expect(core.args['setPresence'], false);
    expect(dotColour(tester, dot), scheme.error);
  });

  // spec: presence/presence-indicator-on-avatars
  testWidgets('a received PresenceUpdate colours the peer\'s avatar dot green '
      'or red across the chat list', (tester) async {
    final core = aliceInGroup()..dmChats = [dmWithBob()];
    await openHome(tester, core);

    // Before any PresenceUpdate, Bob's status is unknown: no dot anywhere.
    expect(find.byKey(const Key('presence-dot')), findsNothing);

    // Bob set Online: his DM row (and his contact row) show a green dot.
    core.setPeerPresence(bob, PresenceStatus.online);
    await settle(tester);
    final dots = find.byKey(const Key('presence-dot'));
    expect(dots, findsWidgets);
    for (final element in dots.evaluate()) {
      final container = element.widget as Container;
      expect((container.decoration! as ShapeDecoration).color, _green);
    }

    // Bob set Offline: the dots turn to the error colour.
    final scheme = Theme.of(
      tester.element(find.byKey(const Key('bottom-nav'))),
    ).colorScheme;
    core.setPeerPresence(bob, PresenceStatus.offline);
    await settle(tester);
    for (final element in find.byKey(const Key('presence-dot')).evaluate()) {
      final container = element.widget as Container;
      expect((container.decoration! as ShapeDecoration).color, scheme.error);
    }
  });

  // spec: presence/presence-indicator-on-avatars
  testWidgets('a peer whose status is unknown shows no presence dot', (
    tester,
  ) async {
    final core = aliceInGroup()..dmChats = [dmWithBob()];
    await openHome(tester, core);
    // No PresenceUpdate has been received for any peer.
    expect(find.byKey(const Key('presence-dot')), findsNothing);
  });
}
