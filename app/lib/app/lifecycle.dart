// app-ui: Foreground and background (flutter-app design D9). The engine
// runs while the app is in the foreground: it stops when the app goes to
// the background and restarts on return, so it reconnects at once and
// catches up in order instead of waiting out a reconnect backoff.
// A closed app is woken by the Pi's "New message" notification (app-ui:
// Push notifications): tapping it opens the app, which starts the core
// here; opening the app removes the shown notifications.
//
// It also follows the phone's network (qa-fixes design D6, "Network
// back"): on any change to a connection it calls `reconnectNow`, so the
// core reconnects at once instead of waiting out its backoff.
import 'dart:async';

import 'package:flutter/widgets.dart';

import '../core/core_service.dart';
import '../push/push_platform.dart';

/// Stops the core when the app is paused and restarts it on resume;
/// tells it to reconnect when the network changes.
class CoreLifecycle extends StatefulWidget {
  const CoreLifecycle({
    super.key,
    required this.core,
    required this.child,
    this.networkChanges,
    this.push,
  });

  final CoreService core;
  final Widget child;

  /// The phone's network changes: whether any connection (Wi-Fi, mobile,
  /// ...) is up after each. `connectivity_plus` in the app; widget tests
  /// pass their own or none.
  final Stream<bool>? networkChanges;

  /// The phone's push service: its shown notifications go away when the app
  /// opens or comes back. Null in widget tests that do not need it.
  final PushPlatformApi? push;

  @override
  State<CoreLifecycle> createState() => _CoreLifecycleState();
}

class _CoreLifecycleState extends State<CoreLifecycle> {
  late final AppLifecycleListener _listener;
  StreamSubscription<bool>? _network;
  bool _stoppedInBackground = false;

  @override
  void initState() {
    super.initState();
    _listener = AppLifecycleListener(onPause: _paused, onResume: _resumed);
    _network = widget.networkChanges?.listen(_networkChanged);
    _clearNotifications();
  }

  void _clearNotifications() {
    widget.push?.clearNotifications().catchError((Object _) {});
  }

  /// A connection came up or changed: no point retrying with none at all.
  void _networkChanged(bool online) {
    if (online) widget.core.reconnectNow();
  }

  void _paused() {
    if (!widget.core.running) return;
    _stoppedInBackground = true;
    widget.core.stop();
  }

  void _resumed() {
    _clearNotifications();
    if (!_stoppedInBackground) return;
    _stoppedInBackground = false;
    widget.core.start();
  }

  @override
  void dispose() {
    _network?.cancel();
    _listener.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
