// app-ui: Foreground and background (flutter-app design D9). The engine
// runs while the app is in the foreground: it stops when the app goes to
// the background and restarts on return, so it reconnects at once and
// catches up in order instead of waiting out a reconnect backoff.
// A closed app is woken by the server's "New message" notification (app-ui:
// Push notifications): tapping it opens the app, which starts the core
// here; opening the app removes the shown notifications.
//
// Going to the background with a message still sending does not stop the core
// at once: it keeps running until the message is accepted or failed, or one
// minute after it connected, whichever comes first (app-ui: Finish sends in the
// background; change finish-sends-in-background).
//
// It also follows the phone's network (qa-fixes design D6, "Network
// back"): on any change to a connection it calls `reconnectNow`, so the
// core reconnects at once instead of waiting out its backoff.
import 'dart:async';

import 'package:flutter/widgets.dart';

import '../core/core_service.dart';
import '../push/push_platform.dart';
import 'background_task.dart';

/// How long the core may keep running in the background, counted from when it
/// connected, to finish sending (app-ui: Finish sends in the background).
const kBackgroundSendLimit = Duration(minutes: 1);

/// How long the core may take to connect in the background before the app stops
/// it anyway.
const kBackgroundConnectWait = Duration(minutes: 1);

/// Stops the core when the app is paused (after finishing any unsent message)
/// and restarts it on resume; tells it to reconnect when the network changes.
class CoreLifecycle extends StatefulWidget {
  const CoreLifecycle({
    super.key,
    required this.core,
    required this.child,
    this.networkChanges,
    this.push,
    this.background,
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

  /// The phone's short background time (iOS); null in widget tests that do not
  /// need it.
  final BackgroundTaskApi? background;

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

  /// Counts pauses, so a wait that was cancelled by a resume (or replaced by
  /// a later pause) does nothing when its futures complete.
  int _pause = 0;
  final List<StreamSubscription<Object?>> _waiting = [];
  Timer? _connectCap;
  Timer? _sendCap;
  VoidCallback? _connectionListener;

  void _paused() {
    if (!widget.core.running) return;
    _cancelWait();
    _stoppedInBackground = true;
    final pause = ++_pause;
    // iOS suspends at once unless asked for time, so ask before anything else.
    unawaited(widget.background?.begin());
    unawaited(_decide(pause));
  }

  /// Stops the core now, or keeps it while an own message is still sending.
  Future<void> _decide(int pause) async {
    var unsent = false;
    try {
      unsent = await widget.core.hasUnsent();
    } catch (_) {
      // Cannot tell: stop, as before.
    }
    if (pause != _pause) return;
    if (!unsent) {
      _finish();
      return;
    }
    // If the phone is already locked, the core's NSFileProtectionComplete
    // database is unreadable; keeping the core running would fault the moment
    // its background tick reads the database (change `ios-stop-core-when-locked`).
    // Stop now instead of waiting; the send waits for the next unlock.
    var available = true;
    try {
      available = await widget.background?.protectedDataAvailable() ?? true;
    } catch (_) {
      // Cannot tell: wait, as before.
    }
    if (pause != _pause) return;
    if (!available) {
      _finish();
      return;
    }
    _wait(pause);
  }

  /// Keeps the core running until the sends are done, the minute after it
  /// connected is up, or it does not connect in time.
  void _wait(int pause) {
    final core = widget.core;
    _waiting.add(
      core.events.listen((event) {
        if (event is CoreEvent_SendStatus) unawaited(_recheck(pause));
      }),
    );
    final expired = widget.background?.expired.listen((_) {
      if (pause == _pause) _finish();
    });
    if (expired != null) _waiting.add(expired);
    // The phone locked while finishing the send: stop the core before its
    // background tick reads the now-unreadable NSFileProtectionComplete database
    // (change `ios-stop-core-when-locked`).
    final locked = widget.background?.locked.listen((_) {
      if (pause == _pause) _finish();
    });
    if (locked != null) _waiting.add(locked);
    _connectCap = Timer(kBackgroundConnectWait, () {
      if (pause == _pause) _finish();
    });
    void startMinute() {
      if (_sendCap != null ||
          core.connection.value != ConnectionStatus.connected) {
        return;
      }
      _connectCap?.cancel();
      _sendCap = Timer(kBackgroundSendLimit, () {
        if (pause == _pause) _finish();
      });
    }

    _connectionListener = startMinute;
    core.connection.addListener(startMinute);
    startMinute();
  }

  Future<void> _recheck(int pause) async {
    var unsent = true;
    try {
      unsent = await widget.core.hasUnsent();
    } catch (_) {
      // Cannot tell: keep waiting; the caps end it.
    }
    if (pause == _pause && !unsent) _finish();
  }

  void _cancelWait() {
    for (final s in _waiting) {
      s.cancel();
    }
    _waiting.clear();
    _connectCap?.cancel();
    _sendCap?.cancel();
    _connectCap = null;
    _sendCap = null;
    final listener = _connectionListener;
    if (listener != null) widget.core.connection.removeListener(listener);
    _connectionListener = null;
  }

  /// The wait is over: stop the core and give the background time back.
  void _finish() {
    _cancelWait();
    _pause++;
    widget.core.stop();
    unawaited(widget.background?.end());
  }

  void _resumed() {
    // Back in the app: any wait ends and the core keeps running.
    final waited = _waiting.isNotEmpty || _connectCap != null;
    _cancelWait();
    _pause++;
    if (waited) unawaited(widget.background?.end());
    _clearNotifications();
    if (!_stoppedInBackground) return;
    _stoppedInBackground = false;
    if (!widget.core.running) widget.core.start();
  }

  @override
  void dispose() {
    _cancelWait();
    _network?.cancel();
    _listener.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
