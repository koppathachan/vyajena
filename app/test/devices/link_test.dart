// app-ui: Link a device.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

void main() {
  // spec: app-ui/link-a-device
  testWidgets('Alice links her tablet: the tablet shows its link request, '
      'her phone approves it, the tablet takes the approval and joins', (
    tester,
  ) async {
    // The tablet: no user yet.
    final tablet = FakeCoreService()..newUser = alice;
    await openApp(tester, tablet);
    await tap(tester, find.textContaining('link this device'));
    expect(tablet.calls, contains('linkRequest'));
    expect(find.text('vyajena-link:request'), findsOneWidget);

    await tester.enterText(
      find.byKey(const Key('approval')),
      'vyajena-approval:approval',
    );
    await tap(tester, find.widgetWithText(FilledButton, 'Link'));
    expect(tablet.args['completeLink'], 'vyajena-approval:approval');
    expect(tablet.state.value, CoreState.ready);
    // Back on the home screen, in the group as Alice's second device.
    expect(find.byKey(const Key('composer')), findsOneWidget);
    expect(find.text('alice@pi'), findsOneWidget);
  });

  // spec: app-ui/link-a-device
  testWidgets('the identity device approves a link request and shows the '
      'approval', (tester) async {
    final shared = recordShares(tester);
    final phone = FakeCoreService(storedUser: alice);
    await openApp(tester, phone);
    await menu(tester, 'Link a new device');
    await tester.enterText(
      find.byKey(const Key('request')),
      'vyajena-link:request',
    );
    await tap(tester, find.text('Approve'));
    expect(phone.args['approveLink'], 'vyajena-link:request');
    expect(find.text('vyajena-approval:approval'), findsOneWidget);
    await tap(tester, find.widgetWithText(FilledButton, 'Share'));
    expect(shared, ['vyajena-approval:approval']);
  });

  // spec: app-ui/link-a-device
  testWidgets('a device without the identity key says so', (tester) async {
    final laptop = FakeCoreService(storedUser: alice)
      ..fails['approveLink'] = const CoreError(
        kind: CoreErrorKind.notIdentityDevice,
        message: 'this device does not hold the identity key',
      );
    await openApp(tester, laptop);
    await menu(tester, 'Link a new device');
    await tester.enterText(
      find.byKey(const Key('request')),
      'vyajena-link:request',
    );
    await tap(tester, find.text('Approve'));
    expect(
      find.text('Only the phone you joined with can link devices.'),
      findsOneWidget,
    );
  });

  // spec: app-ui/link-a-device
  testWidgets('a wrong approval keeps the tablet unlinked and says why', (
    tester,
  ) async {
    final tablet = FakeCoreService()
      ..fails['completeLink'] = const CoreError(
        kind: CoreErrorKind.invalidPayload,
        message: 'this is an invite, not a link approval',
      );
    await openApp(tester, tablet);
    await tap(tester, find.textContaining('link this device'));
    await tester.enterText(
      find.byKey(const Key('approval')),
      'vyajena-invite:x',
    );
    await tap(tester, find.widgetWithText(FilledButton, 'Link'));
    expect(find.text('this is an invite, not a link approval'), findsOneWidget);
    expect(tablet.state.value, CoreState.unregistered);
  });
}
