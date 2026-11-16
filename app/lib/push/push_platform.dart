// The phone's push service behind one small interface (push design D8):
// Firebase Messaging on Android, APNs through UNUserNotificationCenter on
// iOS (no Firebase on iOS), both over the `vyajena/push` method channel.
// Widget tests use a fake (test/push/).
import 'dart:async';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import '../core/core_service.dart';

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

  /// Removes the app's shown notifications ("New message").
  Future<void> clearNotifications();
}

/// The real one, over `vyajena/push`.
class MethodChannelPush implements PushPlatformApi {
  MethodChannelPush() {
    _channel.setMethodCallHandler((call) async {
      if (call.method == 'tokenChanged' && call.arguments is String) {
        _changes.add(call.arguments as String);
      }
    });
  }

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
}
