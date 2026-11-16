// app-ui: Push notifications. The app registers its token through the core on
// each connect and on rotation, asks for permission at the first connect
// (never before joining), clears the token when notifications are not
// allowed, forgets it when removed, and clears shown notifications when the
// app opens or comes back.
import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/app/app.dart';
import 'package:vyajena/core/core_service.dart';
import 'package:vyajena/push/push_platform.dart';
import 'package:vyajena/push/push_service.dart';

import '../support/fake_core_service.dart';

class FakePush implements PushPlatformApi {
  FakePush({this.allow = true, this.current = 'tok-1'});

  bool allow;
  String? current;
  int asked = 0;
  final List<String> calls = [];
  final _changes = StreamController<String>.broadcast();

  void rotate(String token) {
    current = token;
    _changes.add(token);
  }

  @override
  PushPlatform get platform => PushPlatform.fcm;

  @override
  Future<bool> ensurePermission() async {
    asked++;
    return allow;
  }

  @override
  Future<String?> token() async => current;

  @override
  Stream<String> get tokenChanges => _changes.stream;

  @override
  Future<void> deleteToken() async => calls.add('deleteToken');

  @override
  Future<void> clearNotifications() async => calls.add('clearNotifications');
}

const bob = UserRef(username: 'bob', serverId: 'pi');

Future<(FakeCoreService, FakePush, PushService)> started({
  bool allow = true,
}) async {
  final core = FakeCoreService(storedUser: bob);
  await core.open();
  final push = FakePush(allow: allow);
  final service = PushService(core: core, api: push)..start();
  return (core, push, service);
}

void connect(FakeCoreService core) =>
    core.emit(const CoreEvent.connection(status: ConnectionStatus.connected));

void disconnect(FakeCoreService core) => core.emit(
  const CoreEvent.connection(status: ConnectionStatus.disconnected),
);

List<Object?> pushCalls(FakeCoreService core) => [
  for (final c in core.calls)
    if (c == 'setPushToken' || c == 'clearPushToken') c,
];

void main() {
  // spec: app-ui/push-notifications
  test('Bob joins: asked at the first connect, token registered', () async {
    final (core, push, service) = await started();
    await service.idle;
    expect(push.asked, 0, reason: 'not before the core connects');
    expect(pushCalls(core), isEmpty);

    connect(core);
    await service.idle;
    expect(push.asked, 1);
    expect(core.calls.where((c) => c == 'setPushToken'), hasLength(1));
    expect(core.args['setPushToken'], (PushPlatform.fcm, 'tok-1'));

    // Reconnecting in the same process sends nothing new.
    disconnect(core);
    connect(core);
    await service.idle;
    expect(core.calls.where((c) => c == 'setPushToken'), hasLength(1));
  });

  // spec: app-ui/push-notifications
  test('Token rotation on Android: the new token goes to the core', () async {
    final (core, push, service) = await started();
    connect(core);
    await service.idle;
    push.rotate('tok-2');
    await Future<void>.delayed(Duration.zero);
    await service.idle;
    expect(core.args['setPushToken'], (PushPlatform.fcm, 'tok-2'));

    // Rotated while offline: sent at the next connect.
    disconnect(core);
    push.rotate('tok-3');
    await Future<void>.delayed(Duration.zero);
    await service.idle;
    expect(core.args['setPushToken'], (PushPlatform.fcm, 'tok-2'));
    connect(core);
    await service.idle;
    expect(core.args['setPushToken'], (PushPlatform.fcm, 'tok-3'));
  });

  // spec: app-ui/push-notifications
  test('Offline when sending: retried on the next connect', () async {
    final (core, _, service) = await started();
    core.fails['setPushToken'] = const CoreError(
      kind: CoreErrorKind.offline,
      message: 'offline',
    );
    connect(core);
    await service.idle;
    core.fails.remove('setPushToken');
    disconnect(core);
    connect(core);
    await service.idle;
    expect(core.calls.where((c) => c == 'setPushToken'), hasLength(2));
  });

  // spec: app-ui/push-notifications
  test('Permission denied: the token is cleared once, nothing sent', () async {
    final (core, push, service) = await started(allow: false);
    connect(core);
    await service.idle;
    disconnect(core);
    connect(core);
    await service.idle;
    expect(pushCalls(core), ['clearPushToken']);
    expect(push.calls, isNot(contains('deleteToken')));

    // Turned on later in Settings: the next connect registers.
    push.allow = true;
    disconnect(core);
    connect(core);
    await service.idle;
    expect(pushCalls(core), ['clearPushToken', 'setPushToken']);
  });

  // spec: app-ui/push-notifications
  test('Removed device: the platform token is deleted, nothing sent', () async {
    final (core, push, service) = await started();
    connect(core);
    await service.idle;
    core.emit(const CoreEvent.removed());
    await Future<void>.delayed(Duration.zero);
    await service.idle;
    expect(push.calls, contains('deleteToken'));
    push.rotate('tok-after');
    await Future<void>.delayed(Duration.zero);
    await service.idle;
    expect(core.calls.where((c) => c == 'setPushToken'), hasLength(1));
  });

  // spec: app-ui/push-notifications
  testWidgets('Opening or resuming the app clears shown notifications', (
    tester,
  ) async {
    final core = FakeCoreService(storedUser: bob);
    final push = FakePush();
    await tester.pumpWidget(VyajenaApp(core: core, push: push));
    await core.open();
    await tester.pump();
    expect(push.calls, ['clearNotifications']);

    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.paused);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await tester.pump();
    expect(push.calls, ['clearNotifications', 'clearNotifications']);
  });
}
