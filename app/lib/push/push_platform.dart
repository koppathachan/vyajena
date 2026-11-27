// The phone's push service behind one small interface (push design D8):
// Firebase Messaging on Android, APNs through UNUserNotificationCenter on
// iOS (no Firebase on iOS), both over the `vyajena/push` method channel.
// Widget tests use a fake (test/push/).
import 'dart:async';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import '../core/core_service.dart';
import 'notification_text.dart';

/// The user tapped a notification (app-ui: A notification opens its
/// conversation).
class NotificationTap {
  const NotificationTap({this.chatId});

  /// The conversation the notification was for; null opens the chat list.
  final int? chatId;

  @override
  bool operator ==(Object other) =>
      other is NotificationTap && other.chatId == chatId;

  @override
  int get hashCode => chatId.hashCode;
}

/// What the app needs from the phone's push service.
abstract class PushPlatformApi {
  /// FCM on Android, APNs on iOS.
  PushPlatform get platform;

  /// Asks for notification permission the first time (once per install; the
  /// native side remembers), then reports whether notifications are allowed.
  /// False without push support (an Android build without Firebase config).
  Future<bool> ensurePermission();

  /// This device's current token, or null if there is none yet.
  Future<String?> token();

  /// A new token whenever the platform rotates it.
  Stream<String> get tokenChanges;

  /// Forgets this device's token (the device was removed).
  Future<void> deleteToken();

  /// Removes the app's shown notifications.
  Future<void> clearNotifications();

  /// Removes the notification of one conversation (app-ui: A notification
  /// opens its conversation: it is cleared once on screen).
  Future<void> clearConversation(int chatId);

  /// Posts the local notifications built from the core's summary (Android; a
  /// no-op on iOS, where the system shows the server's alert and the
  /// Notification Service Extension rewrites it). The same words on both
  /// platforms (local-notifications: The device builds the notification
  /// locally).
  Future<void> showNotifications(RenderedNotifications notifications);

  /// Saves what the Notification Service Extension shows when a wake arrives
  /// (iOS: the App Group; a no-op on Android), and sets the app-icon badge.
  /// Called with [RenderedNotifications.empty] when nothing is unread.
  Future<void> saveSnapshot(RenderedNotifications notifications);

  /// A notification was tapped while the app runs.
  Stream<NotificationTap> get taps;

  /// The tap that launched the app, if one did (read once).
  Future<NotificationTap?> takeLaunchTap();

  /// The app's answer to a data wake the phone delivered (Android): catch up
  /// and post the notifications. Null while nothing handles it.
  set wakeHandler(Future<void> Function()? handler);

  /// Tells the phone the wake was handled, so it may stop the work it started
  /// for it (Android).
  Future<void> wakeDone();
}

/// The real one, over `vyajena/push`.
class MethodChannelPush implements PushPlatformApi {
  MethodChannelPush() {
    _channel.setMethodCallHandler((call) async {
      if (call.method == 'tokenChanged' && call.arguments is String) {
        _changes.add(call.arguments as String);
      } else if (call.method == 'notificationTapped') {
        final id = call.arguments;
        _taps.add(NotificationTap(chatId: id is int ? id : null));
      } else if (call.method == 'wake') {
        await _wake?.call();
      }
    });
  }

  Future<void> Function()? _wake;
  final _taps = StreamController<NotificationTap>.broadcast();

  static const _channel = MethodChannel('vyajena/push');
  final _changes = StreamController<String>.broadcast();

  @override
  PushPlatform get platform => defaultTargetPlatform == TargetPlatform.iOS
      ? PushPlatform.apns
      : PushPlatform.fcm;

  @override
  Future<bool> ensurePermission() async =>
      await _channel.invokeMethod<bool>('ensurePermission') ?? false;

  @override
  Future<String?> token() => _channel.invokeMethod<String>('token');

  @override
  Stream<String> get tokenChanges => _changes.stream;

  @override
  Future<void> deleteToken() => _channel.invokeMethod<void>('deleteToken');

  @override
  Future<void> clearNotifications() =>
      _channel.invokeMethod<void>('clearNotifications');

  @override
  Future<void> clearConversation(int chatId) =>
      _channel.invokeMethod<void>('clearConversation', chatId);

  /// What crosses the channel: the words and ids only.
  static Map<String, Object?> _encode(RenderedNotifications n) => {
    'lines': [
      for (final l in n.lines)
        {'key': l.key, 'text': l.text, 'chatId': l.chatId},
    ],
    'combined': n.combined,
    'badge': n.badge,
    'singleChatId': n.singleChatId,
  };

  @override
  Future<void> showNotifications(RenderedNotifications notifications) =>
      _channel.invokeMethod<void>('showNotifications', _encode(notifications));

  @override
  Future<void> saveSnapshot(RenderedNotifications notifications) =>
      _channel.invokeMethod<void>('saveSnapshot', _encode(notifications));

  @override
  Stream<NotificationTap> get taps => _taps.stream;

  @override
  Future<NotificationTap?> takeLaunchTap() async {
    final tap = await _channel.invokeMethod<Map<Object?, Object?>>(
      'takeLaunchTap',
    );
    if (tap == null) return null;
    final id = tap['chatId'];
    return NotificationTap(chatId: id is int ? id : null);
  }

  @override
  set wakeHandler(Future<void> Function()? handler) => _wake = handler;

  @override
  Future<void> wakeDone() => _channel.invokeMethod<void>('wakeDone');
}
