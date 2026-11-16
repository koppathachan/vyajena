// app-ui: Push notifications (push design D9, D10). Registers this device's
// push token through the core each time the core connects and when the
// platform rotates it; asks for notification permission once, at the first
// connect after joining (never before: an unregistered device never
// connects); clears the token when notifications are not allowed; forgets
// it when the device is removed. Nothing here sees message text: the
// notification is the Pi's fixed "New message".
import 'dart:async';

import 'package:flutter/foundation.dart';

import '../core/core_service.dart';
import 'push_platform.dart';

/// Keeps the Pi's copy of this device's push token right.
class PushService {
  PushService({required this.core, required this.api});

  final CoreService core;
  final PushPlatformApi api;

  /// The token the Pi has from this process, if any (memory only).
  String? _sent;

  /// Whether this process already cleared the token on the Pi.
  bool _cleared = false;
  StreamSubscription<String>? _tokens;
  StreamSubscription<CoreEvent>? _events;
  Future<void> _queue = Future.value();

  /// Starts following the core's connection, its events and token changes.
  void start() {
    core.connection.addListener(_connectionChanged);
    _tokens = api.tokenChanges.listen((token) => _run(() => _sync(token)));
    _events = core.events.listen((event) {
      if (event is CoreEvent_Removed) _run(_removed);
    });
    _connectionChanged();
  }

  /// Stops following.
  Future<void> dispose() async {
    core.connection.removeListener(_connectionChanged);
    await _tokens?.cancel();
    await _events?.cancel();
  }

  /// Done when every queued step has run (tests).
  @visibleForTesting
  Future<void> get idle => _queue;

  void _connectionChanged() {
    if (core.connection.value == ConnectionStatus.connected) {
      _run(() => _sync(null));
    }
  }

  /// One step at a time, in order; a failing step never stops the next.
  void _run(Future<void> Function() step) {
    _queue = _queue.then((_) => step()).catchError((Object _) {});
  }

  Future<void> _sync(String? rotated) async {
    if (core.removed.value) return;
    if (core.connection.value != ConnectionStatus.connected) return;
    final allowed = await api.ensurePermission();
    if (!allowed) {
      if (_cleared) return;
      await core.clearPushToken();
      _cleared = true;
      _sent = null;
      return;
    }
    final token = rotated ?? await api.token();
    if (token == null || token == _sent) return;
    await core.setPushToken(api.platform, token);
    _sent = token;
    _cleared = false;
  }

  Future<void> _removed() async {
    _sent = null;
    await api.deleteToken();
  }
}
