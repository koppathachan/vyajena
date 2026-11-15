// app-ui: Foreground and background (flutter-app design D9). The engine
// runs while the app is in the foreground: it stops when the app goes to
// the background and restarts on return, so it reconnects at once and
// catches up in order instead of waiting out a reconnect backoff.
// Background execution waits for real push.
//
// It also follows the phone's network (qa-fixes design D6, "Network
// back"): on any change to a connection it calls `reconnectNow`, so the
// core reconnects at once instead of waiting out its backoff.
import 'dart:async';

import 'package:flutter/widgets.dart';

import '../core/core_service.dart';

/// Stops the core when the app is paused and restarts it on resume;
/// tells it to reconnect when the network changes.
class CoreLifecycle extends StatefulWidget {
  const CoreLifecycle({
    super.key,
    required this.core,
    required this.child,
    this.networkChanges,
  });

  final CoreService core;
  final Widget child;

  /// The phone's network changes: whether any connection (Wi-Fi, mobile,
  /// ...) is up after each. `connectivity_plus` in the app; widget tests
  /// pass their own or none.
  final Stream<bool>? networkChanges;

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
