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
  testWidgets('Bob joins: he sees bob@chat on 127.0.0.1:8443 first, then '
      'registers and lands in the chat', (tester) async {
    final core = await openApp(tester);
    expect(find.text('Join the group'), findsOneWidget);

    await paste(tester, 'vyajena-invite:bob');
    expect(find.text('bob@chat'), findsOneWidget);
    expect(find.text('on 127.0.0.1:8443'), findsOneWidget);
    expect(core.calls, isNot(contains('register')));

    await tap(tester, find.text('Join'));
    expect(core.calls, containsAllInOrder(['parseInvite', 'register']));
    expect(core.args['register'], 'vyajena-invite:bob');
    expect(core.state.value, CoreState.ready);
    // The app lands on the chat list; open the group to reach its chat.
    await enterGroup(tester);
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
    await enterGroup(tester);
    expect(find.byKey(const Key('composer')), findsOneWidget);
  });

  // spec: app-ui/join-from-an-invite
  testWidgets('Welcome refused while waiting: the waiting screen shows the '
      'owner\'s text with the reason and when, and the chat once the device '
      'joins', (tester) async {
    final core = FakeCoreService()..epochValue = null;
    await openApp(tester, core);
    await paste(tester, 'vyajena-invite:bob');
    await tap(tester, find.text('Join'));
    expect(find.byKey(const Key('refusal')), findsNothing);

    core.emit(
      const CoreEvent.commitRefused(
        reason: 'a Welcome could not be used: no matching KeyPackage',
      ),
    );
    await settle(tester);
    expect(find.text(waitingText), findsNothing);
    expect(find.text(refusedText), findsOneWidget);
    // "Reason: <error> (<time>)", the time as the chat shows times.
    expect(
      find.textContaining(
        RegExp(
          r'^Reason: a Welcome could not be used: no matching KeyPackage '
          r'\(.*\d:\d\d.*\)$',
        ),
      ),
      findsOneWidget,
    );

    core.epochValue = BigInt.one;
    core.emit(const CoreEvent.joinedGroup());
    await settle(tester);
    expect(find.byKey(const Key('refusal')), findsNothing);
    await enterGroup(tester);
    expect(find.byKey(const Key('composer')), findsOneWidget);
  });

  // spec: app-ui/join-from-an-invite
  testWidgets('Asked to be added again: the owner\'s text, or that it waits '
      'for the admin after the daily limit', (tester) async {
    final core = FakeCoreService()..epochValue = null;
    await openApp(tester, core);
    await paste(tester, 'vyajena-invite:bob');
    await tap(tester, find.text('Join'));

    // Answers that change nothing it waits for show nothing.
    core.emit(const CoreEvent.readdRequested(outcome: ReaddAnswer.notAMember));
    await settle(tester);
    expect(find.byKey(const Key('refusal')), findsNothing);
    expect(find.text(waitingText), findsOneWidget);

    // Stuck from an older build: no refusal on record, only the ask.
    core.emit(const CoreEvent.readdRequested(outcome: ReaddAnswer.asked));
    await settle(tester);
    expect(find.text(refusedText), findsOneWidget);
    expect(find.text(readdAskedText), findsOneWidget);

    core.emit(
      const CoreEvent.commitRefused(reason: 'a Welcome could not be used: x'),
    );
    core.emit(
      const CoreEvent.readdRequested(outcome: ReaddAnswer.limitReached),
    );
    await settle(tester);
    expect(find.text(readdLimitText), findsOneWidget);
    expect(find.text(readdAskedText), findsNothing);
    expect(
      find.textContaining('Reason: a Welcome could not be used: x ('),
      findsOneWidget,
    );

    core.epochValue = BigInt.one;
    core.emit(const CoreEvent.joinedGroup());
    await settle(tester);
    expect(find.byKey(const Key('refusal')), findsNothing);
    await enterGroup(tester);
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

  // spec: app-ui/invite-expiry-is-surfaced
  testWidgets('pasting an expired invite says it expired and a new one is '
      'needed, not a generic failure', (tester) async {
    final core = FakeCoreService()
      ..fails['register'] = const CoreError(
        kind: CoreErrorKind.inviteExpired,
        message: 'the invite has expired',
      );
    await openApp(tester, core);
    await paste(tester, 'vyajena-invite:bob');
    await tap(tester, find.text('Join'));
    expect(
      find.text('This invite has expired. Ask the admin for a fresh one.'),
      findsOneWidget,
    );
    expect(find.textContaining('Could not join'), findsNothing);
  });

  // spec: app-ui/join-from-an-invite
  testWidgets('Bad invite text: the app says the invite is not valid and '
      'calls nothing', (tester) async {
    final core = await openApp(tester);
    await paste(tester, 'hello, this is not an invite');
    expect(find.textContaining('This is not a valid invite'), findsOneWidget);
    expect(find.text('Join'), findsNothing);
    // Only the local parse; nothing that reaches the server (the onboarding
    // probe only runs for a valid handle).
    expect(core.calls, ['open', 'parseInvite']);
  });

  // dev-onboarding.

  FakeCoreService openCore({bool joinCodeRequired = false}) => FakeCoreService()
    ..onboarding = ServerOnboarding(
      serverId: 'demo',
      open: true,
      joinCodeRequired: joinCodeRequired,
      maxUsers: 10,
      adminUsername: 'akhil',
    );

  // spec: app-ui/simplified-onboarding-in-open-mode
  testWidgets('open mode: a server handle shows the username screen and '
      'registers openly with the typed username', (tester) async {
    final core = openCore();
    await openApp(tester, core);
    await paste(tester, 'vyajena-invite:handle');

    // The simplified screen, not the paste-invite preview.
    expect(find.byKey(const Key('username')), findsOneWidget);
    expect(find.text('Join as the admin (akhil)'), findsOneWidget);
    expect(find.byKey(const Key('joinCode')), findsNothing);

    await tester.enterText(find.byKey(const Key('username')), 'reviewer');
    await tap(tester, find.text('Join'));
    expect(core.calls, containsAllInOrder(['serverOnboarding', 'registerOpen']));
    expect(core.args['registerOpen'], 'reviewer/');
    expect(core.state.value, CoreState.ready);
  });

  // spec: app-ui/simplified-onboarding-in-open-mode
  testWidgets('open mode: "join as the admin" registers the admin username', (
    tester,
  ) async {
    final core = openCore();
    await openApp(tester, core);
    await paste(tester, 'vyajena-invite:handle');
    await tap(tester, find.text('Join as the admin (akhil)'));
    expect(core.args['registerOpen'], 'akhil/');
    expect(core.state.value, CoreState.ready);
  });

  // spec: app-ui/simplified-onboarding-in-open-mode
  testWidgets('open mode: a required join code shows a join-code field', (
    tester,
  ) async {
    final core = openCore(joinCodeRequired: true);
    await openApp(tester, core);
    await paste(tester, 'vyajena-invite:handle');
    expect(find.byKey(const Key('joinCode')), findsOneWidget);
    await tester.enterText(find.byKey(const Key('username')), 'reviewer');
    await tester.enterText(find.byKey(const Key('joinCode')), 'letmein');
    await tap(tester, find.text('Join'));
    expect(core.args['registerOpen'], 'reviewer/letmein');
  });

  // spec: app-ui/simplified-onboarding-in-open-mode
  testWidgets('invite mode shows the paste-invite preview, not a username '
      'field', (tester) async {
    final core = await openApp(tester); // default fake: invite mode
    await paste(tester, 'vyajena-invite:bob');
    expect(find.byKey(const Key('username')), findsNothing);
    expect(find.text('bob@chat'), findsOneWidget);
    await tap(tester, find.text('Join'));
    expect(core.calls, isNot(contains('registerOpen')));
    expect(core.calls, contains('register'));
  });
}
