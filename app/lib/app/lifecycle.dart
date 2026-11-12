// app-ui: Foreground and background (flutter-app design D9). The engine
// runs while the app is in the foreground: it stops when the app goes to
// the background and restarts on return, so it reconnects at once and
// catches up in order instead of waiting out a reconnect backoff.
// Background execution waits for real push.
import 'package:flutter/widgets.dart';

import '../core/core_service.dart';

/// Stops the core when the app is paused and restarts it on resume.
class CoreLifecycle extends StatefulWidget {
  const CoreLifecycle({super.key, required this.core, required this.child});

  final CoreService core;
  final Widget child;

  @override
  State<CoreLifecycle> createState() => _CoreLifecycleState();
}

class _CoreLifecycleState extends State<CoreLifecycle> {
  late final AppLifecycleListener _listener;
  bool _stoppedInBackground = false;

  @override
  void initState() {
    super.initState();
    _listener = AppLifecycleListener(onPause: _paused, onResume: _resumed);
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
    _listener.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
