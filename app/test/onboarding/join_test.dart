// app-ui: Join from an invite.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/chat/group_gate.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

Future<void> paste(WidgetTester tester, String text) async {
  await tester.enterText(find.byKey(const Key('invite')), text);
  await tap(tester, find.text('Check invite'));
}

void main() {
  // spec: app-ui/join-from-an-invite
  testWidgets('Bob joins: he sees bob@pi on 127.0.0.1:8443 first, then '
      'registers and lands in the chat', (tester) async {
    final core = await openApp(tester);
    expect(find.text('Join the group'), findsOneWidget);

    await paste(tester, 'vyajena-invite:bob');
    expect(find.text('bob@pi'), findsOneWidget);
    expect(find.text('on 127.0.0.1:8443'), findsOneWidget);
    expect(core.calls, isNot(contains('register')));

    await tap(tester, find.text('Join'));
    expect(core.calls, containsAllInOrder(['parseInvite', 'register']));
    expect(core.args['register'], 'vyajena-invite:bob');
    expect(core.state.value, CoreState.ready);
    // The chat, with its composer.
    expect(find.byKey(const Key('composer')), findsOneWidget);
    expect(find.text('No messages yet'), findsOneWidget);
  });

  // spec: app-ui/join-from-an-invite
  testWidgets('Waiting to be added: the app says it waits for the admin\'s '
      'device and shows the chat once the core emits JoinedGroup', (
    tester,
  ) async {
    final core = FakeCoreService()..epochValue = null;
    await openApp(tester, core);
    await paste(tester, 'vyajena-invite:bob');
    await tap(tester, find.text('Join'));
    expect(find.text(waitingText), findsOneWidget);
    expect(find.byKey(const Key('composer')), findsNothing);

    core.epochValue = BigInt.one;
    core.emit(const CoreEvent.joinedGroup());
    await settle(tester);
    expect(find.text(waitingText), findsNothing);
    expect(find.byKey(const Key('composer')), findsOneWidget);
  });

  // spec: app-ui/join-from-an-invite
  testWidgets('Wrong pin: a server identity error, not a network error, and '
      'no join', (tester) async {
    final core = FakeCoreService()
      ..fails['register'] = const CoreError(
        kind: CoreErrorKind.serverIdentity,
        message: 'the TLS key does not match the pin',
      );
    await openApp(tester, core);
    await paste(tester, 'vyajena-invite:bob');
    await tap(tester, find.text('Join'));
    expect(
      find.textContaining("server's identity does not match"),
      findsOneWidget,
    );
    expect(find.textContaining('Could not reach'), findsNothing);
    expect(core.state.value, CoreState.unregistered);
  });

  // spec: app-ui/join-from-an-invite
  testWidgets('a network error says the server could not be reached', (
    tester,
  ) async {
    final core = FakeCoreService()
      ..fails['register'] = const CoreError(
        kind: CoreErrorKind.network,
        message: 'connection refused',
      );
    await openApp(tester, core);
    await paste(tester, 'vyajena-invite:bob');
    await tap(tester, find.text('Join'));
    expect(
      find.text('Could not reach the server: connection refused'),
      findsOneWidget,
    );
    expect(find.textContaining("server's identity"), findsNothing);
  });

  // spec: app-ui/join-from-an-invite
  testWidgets('Bad invite text: the app says the invite is not valid and '
      'calls nothing', (tester) async {
    final core = await openApp(tester);
    await paste(tester, 'hello, this is not an invite');
    expect(find.textContaining('This is not a valid invite'), findsOneWidget);
    expect(find.text('Join'), findsNothing);
    // Only the local parse; nothing that reaches the Pi.
    expect(core.calls, ['open', 'parseInvite']);
  });
}
