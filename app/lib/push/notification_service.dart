// app-ui: Notification detail level; Type-aware notification wording; A
// notification opens its conversation (change `smarter-notifications`).
//
// The server's push is a content-free wake (message-delivery: Wake push payload
// is content-free and metadata-minimal). Everything a person reads is made here,
// on the device, from the core's summary of types and unread counts
// (local-notifications: The device builds the notification locally):
//
// - Android: the data wake reaches [handleWake]: the core catches up, the summary
//   is rendered at the chosen detail level and posted as local notifications.
// - iOS: the system shows the server's "New message" alert and the Notification
//   Service Extension rewrites it from the snapshot [refreshSnapshot] keeps in
//   the App Group. The extension decrypts nothing and sees no sender.
//
// Nothing here reads message text or a sender: the summary has none.
import 'dart:async';

import 'package:flutter/widgets.dart';

import '../core/core_service.dart';
import 'notification_text.dart';
import 'push_platform.dart';

/// How long the connection stays quiet before a wake counts as caught up.
const kWakeQuiet = Duration(milliseconds: 1200);

/// The most a wake may take: the phone gives a pushed app only a few seconds.
const kWakeLimit = Duration(seconds: 8);

/// Builds and posts the app's local notifications, and opens the conversation
/// a tapped one is for.
class NotificationService {
  NotificationService({
    required this.core,
    required this.api,
    this.onOpen,
    this.isForeground = _lifecycleResumed,
    this.quiet = kWakeQuiet,
    this.limit = kWakeLimit,
  });

  final CoreService core;
  final PushPlatformApi api;

  /// Opens the conversation a tap names (null: the chat list).
  final void Function(NotificationTap tap)? onOpen;

  /// Whether the app is in front (then the core is connected and a notification
  /// would only repeat what is on screen).
  final bool Function() isForeground;

  /// How quiet the connection stays before a wake counts as caught up.
  final Duration quiet;

  /// The most a wake may take.
  final Duration limit;

  StreamSubscription<NotificationTap>? _taps;
  AppLifecycleListener? _lifecycle;
  Future<void> _queue = Future.value();

  static bool _lifecycleResumed() =>
      WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;

  /// Starts following taps and wakes.
  void start() {
    api.wakeHandler = handleWake;
    _taps = api.taps.listen(_tapped);
    // The saved words (and the badge) are brought up to date as the app goes
    // to the background, which is also when a changed detail level matters.
    _lifecycle = AppLifecycleListener(
      onPause: () => unawaited(_seen().then((_) => refreshSnapshot())),
      onResume: () => unawaited(_seen()),
    );
    unawaited(_launchTap());
  }

  Future<void> dispose() async {
    api.wakeHandler = null;
    _lifecycle?.dispose();
    await _taps?.cancel();
  }

  /// Done when every queued step has run (tests).
  @visibleForTesting
  Future<void> get idle => _queue;

  Future<void> _launchTap() async {
    try {
      final tap = await api.takeLaunchTap();
      if (tap != null) _tapped(tap);
    } on Object {
      // No launch tap to read: the app opened normally.
    }
  }

  void _tapped(NotificationTap tap) {
    final chat = tap.chatId;
    if (chat != null) {
      unawaited(api.clearConversation(chat).catchError((Object _) {}));
    }
    onOpen?.call(tap);
  }

  /// The user has been in the app, so its admin-events need no notification.
  Future<void> _seen() async {
    try {
      final events = (await core.notificationSummary()).adminEvents;
      if (events.isNotEmpty) {
        await core.clearAdminEvents(events.map((e) => e.id).reduce(_max));
      }
    } on Object {
      // Not open yet: nothing pending to clear.
    }
  }

  static int _max(int a, int b) => a > b ? a : b;

  /// The words to show now, from the core's summary.
  Future<RenderedNotifications> render() async {
    final summary = await core.notificationSummary();
    return renderNotifications(summary);
  }

  /// Saves what the iOS Notification Service Extension may show, and sets the
  /// app-icon badge. Call it when the app goes to the background, so the saved
  /// words match the core's unread state.
  Future<void> refreshSnapshot() {
    final step = _queue.then((_) async {
      try {
        await api.saveSnapshot(await render());
      } on Object {
        // The extension keeps the generic alert.
      }
    });
    _queue = step;
    return step;
  }

  /// A data wake arrived (Android): catch up, then post the notifications. The
  /// core is started for the wake if it was stopped, and stopped again after.
  /// Does nothing while the app is in front.
  Future<void> handleWake() async {
    if (isForeground()) return;
    final wasRunning = core.running;
    if (core.state.value == CoreState.opening) await core.open();
    if (core.user.value == null) return;
    if (!core.running) await core.start();
    try {
      await _caughtUp();
      final summary = await core.notificationSummary();
      final rendered = renderNotifications(summary);
      if (!rendered.isEmpty) {
        await api.showNotifications(rendered);
        if (summary.adminEvents.isNotEmpty) {
          await core.clearAdminEvents(
            summary.adminEvents.map((e) => e.id).reduce(_max),
          );
        }
      }
    } finally {
      // Back to what it was: a core the wake started is stopped again.
      if (!wasRunning) await core.stop();
    }
  }

  /// Waits until the core is connected and its events have been quiet for
  /// [kWakeQuiet], or [kWakeLimit] has passed.
  Future<void> _caughtUp() {
    final done = Completer<void>();
    Timer? calm;
    StreamSubscription<CoreEvent>? events;
    Timer? timeout;
    late final VoidCallback listener;
    void finish() {
      if (done.isCompleted) return;
      calm?.cancel();
      timeout?.cancel();
      events?.cancel();
      core.connection.removeListener(listener);
      done.complete();
    }

    listener = () {
      calm?.cancel();
      if (core.connection.value == ConnectionStatus.connected) {
        calm = Timer(quiet, finish);
      }
    };
    events = core.events.listen((_) => listener());
    core.connection.addListener(listener);
    timeout = Timer(limit, finish);
    listener();
    return done.future;
  }
}
