// app-ui: Deleting your account; The admin sees no self-delete
// (account-deletion). The Profile tab's "Delete account" action: type-to-confirm
// for a member, greyed-out with a note for the admin.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

final profileTab = find.byKey(const Key('tab-profile'));
final deleteEntry = find.byKey(const Key('settings-delete-account'));
final confirmField = find.byKey(const Key('delete-account-confirm-field'));
final confirmButton = find.byKey(const Key('delete-account-confirm'));

/// A tall screen so the whole Profile tab is built and on screen.
void tall(WidgetTester tester) {
  tester.view.physicalSize = const Size(800, 2400);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
}

/// Opens the Profile tab for [core].
Future<void> openProfile(WidgetTester tester, FakeCoreService core) async {
  tall(tester);
  await openHome(tester, core);
  await tap(tester, profileTab);
}

/// Whether the confirm button is enabled (has an onPressed).
bool confirmEnabled(WidgetTester tester) =>
    tester.widget<FilledButton>(confirmButton).onPressed != null;

void main() {
  // spec: app-ui/the-admin-sees-no-self-delete
  testWidgets('a member has an enabled Delete account action', (tester) async {
    final core = FakeCoreService(storedUser: alice)..admin = false;
    await openProfile(tester, core);
    await tester.ensureVisible(deleteEntry);
    expect(deleteEntry, findsOneWidget);
    expect(tester.widget<ListTile>(deleteEntry).enabled, isTrue);
  });

  // spec: app-ui/the-admin-sees-no-self-delete
  testWidgets('the admin sees Delete account greyed-out with the note', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: akhil)..admin = true;
    await openProfile(tester, core);
    await tester.ensureVisible(deleteEntry);
    expect(tester.widget<ListTile>(deleteEntry).enabled, isFalse);
    expect(
      find.textContaining('admin-reset', findRichText: true),
      findsOneWidget,
    );
    // Tapping it does nothing: no delete is attempted.
    await tester.tap(deleteEntry, warnIfMissed: false);
    await settle(tester);
    expect(core.calls, isNot(contains('deleteAccount')));
    expect(confirmField, findsNothing);
  });

  // spec: app-ui/deleting-your-account
  testWidgets('type-to-confirm gates the delete and then deletes', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: alice)..admin = false;
    await openProfile(tester, core);
    await tap(tester, deleteEntry);

    // The confirmation is up and confirm is disabled until the username matches.
    expect(confirmField, findsOneWidget);
    expect(confirmEnabled(tester), isFalse);

    await tester.enterText(confirmField, 'not-alice');
    await settle(tester);
    expect(confirmEnabled(tester), isFalse, reason: 'a wrong name stays locked');

    await tester.enterText(confirmField, 'alice');
    await settle(tester);
    expect(confirmEnabled(tester), isTrue, reason: 'the exact username unlocks');

    await tap(tester, confirmButton);
    expect(core.calls, contains('deleteAccount'));
    // On success the core returns to the unregistered state → onboarding.
    expect(core.state.value, CoreState.unregistered);
  });

  // spec: app-ui/deleting-your-account
  testWidgets('dismissing the confirmation deletes nothing', (tester) async {
    final core = FakeCoreService(storedUser: alice)..admin = false;
    await openProfile(tester, core);
    await tap(tester, deleteEntry);
    await tap(tester, find.byKey(const Key('delete-account-cancel')));
    expect(core.calls, isNot(contains('deleteAccount')));
    expect(core.state.value, CoreState.ready);
  });

  // spec: app-ui/deleting-your-account
  testWidgets('a failed delete keeps the account and shows the error', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: alice)
      ..admin = false
      ..fails['deleteAccount'] = const CoreError(
        kind: CoreErrorKind.network,
        message: 'the server could not be reached',
      );
    await openProfile(tester, core);
    await tap(tester, deleteEntry);
    await tester.enterText(confirmField, 'alice');
    await settle(tester);
    await tap(tester, confirmButton);

    // The account is kept (still ready, not onboarding) and the error is shown.
    expect(core.state.value, CoreState.ready);
    expect(
      find.textContaining('the server could not be reached'),
      findsOneWidget,
    );
  });
}
