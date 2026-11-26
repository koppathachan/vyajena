// app-ui: Safety and moderation entry; moderation: Published moderation policy
// (change `report-and-block`). The Profile tab's "Safety & moderation" entry
// renders the policy — how to report and block, the 24-hour commitment and the
// contact email (the <SAFETY_CONTACT_EMAIL> placeholder).
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

void main() {
  // spec: app-ui/safety-and-moderation-entry
  // spec: moderation/published-moderation-policy
  testWidgets('the Profile tab opens the moderation policy', (tester) async {
    final core = FakeCoreService(storedUser: alice);
    await openHome(tester, core);
    await tap(tester, find.byKey(const Key('tab-profile')));

    // The entry is at the bottom of the Profile tab's lazy list; scroll to it.
    final entry = find.byKey(const Key('settings-safety'));
    await tester.scrollUntilVisible(
      entry,
      300,
      scrollable: find
          .descendant(
            of: find.byKey(const Key('profile-list')),
            matching: find.byType(Scrollable),
          )
          .first,
    );
    await tap(tester, entry);

    expect(find.byKey(const Key('safety-screen')), findsOneWidget);
    expect(find.textContaining('24 hours'), findsOneWidget);
    expect(find.textContaining('block'), findsWidgets);
    expect(find.textContaining('<SAFETY_CONTACT_EMAIL>'), findsOneWidget);
  });
}
