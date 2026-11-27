// A fake phone push service for widget tests: records what the app asked of it
// and lets a test deliver a token change, a tap or a wake.
import 'dart:async';

import 'package:vyajena/core/core_service.dart';
import 'package:vyajena/push/notification_text.dart';
import 'package:vyajena/push/push_platform.dart';

class FakePush implements PushPlatformApi {
  FakePush({this.allow = true, this.current = 'tok-1'});

  bool allow;
  String? current;
  int asked = 0;
  final List<String> calls = [];

  /// What the app posted as local notifications, newest last (Android).
  final List<RenderedNotifications> shown = [];

  /// What the app saved for the iOS extension, newest last.
  final List<RenderedNotifications> snapshots = [];

  /// The conversations whose notification the app cleared.
  final List<int> clearedConversations = [];

  /// The tap that launched the app, if a test sets one.
  NotificationTap? launchTap;
  bool wakeFinished = false;
  Future<void> Function()? _wake;
  final _changes = StreamController<String>.broadcast();
  final _taps = StreamController<NotificationTap>.broadcast();

  void rotate(String token) {
    current = token;
    _changes.add(token);
  }

  /// The user taps a notification while the app runs.
  void tap(int? chatId) => _taps.add(NotificationTap(chatId: chatId));

  /// The phone delivers a data wake; completes when the app has handled it.
  Future<void> wake() async => await _wake?.call();

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

  @override
  Future<void> clearConversation(int chatId) async =>
      clearedConversations.add(chatId);

  @override
  Future<void> showNotifications(RenderedNotifications notifications) async =>
      shown.add(notifications);

  @override
  Future<void> saveSnapshot(RenderedNotifications notifications) async =>
      snapshots.add(notifications);

  @override
  Stream<NotificationTap> get taps => _taps.stream;

  @override
  Future<NotificationTap?> takeLaunchTap() async {
    final tap = launchTap;
    launchTap = null;
    return tap;
  }

  @override
  set wakeHandler(Future<void> Function()? handler) => _wake = handler;

  @override
  Future<void> wakeDone() async => wakeFinished = true;
}
