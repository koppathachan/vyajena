// The phone's short background time for finishing a send (app-ui: Finish sends
// in the background; change finish-sends-in-background D3). iOS suspends an app
// almost at once unless it asks for time (`beginBackgroundTask`, about 30
// seconds, ended when done or when the system says so); Android keeps the
// process for a while and needs nothing. Both over the `vyajena/background`
// method channel. Widget tests use a fake.
import 'dart:async';

import 'package:flutter/services.dart';

/// What the lifecycle needs from the phone's background time.
abstract class BackgroundTaskApi {
  /// Asks the system for time to keep running after the app is paused.
  Future<void> begin();

  /// Gives the time back: the wait is over.
  Future<void> end();

  /// The system says the time is over: stop at once.
  Stream<void> get expired;

  /// iOS says its protected data became unavailable: the device locked, so the
  /// core's `NSFileProtectionComplete` database is about to be unreadable and the
  /// core must stop before it touches it (change `ios-stop-core-when-locked`).
  /// Never fires on a phone without the native side (Android, tests).
  Stream<void> get locked;

  /// Whether iOS protected data is available now (the device is unlocked). A
  /// phone without the native side (Android, tests) is always available.
  Future<bool> protectedDataAvailable();
}

/// The real one, over `vyajena/background`.
class MethodChannelBackgroundTask implements BackgroundTaskApi {
  MethodChannelBackgroundTask() {
    _channel.setMethodCallHandler((call) async {
      if (call.method == 'expired') _expired.add(null);
      if (call.method == 'locked') _locked.add(null);
    });
  }

  static const _channel = MethodChannel('vyajena/background');
  final _expired = StreamController<void>.broadcast();
  final _locked = StreamController<void>.broadcast();

  @override
  Stream<void> get expired => _expired.stream;

  @override
  Stream<void> get locked => _locked.stream;

  @override
  Future<bool> protectedDataAvailable() async {
    try {
      return await _channel.invokeMethod<bool>('protectedDataAvailable') ?? true;
    } on MissingPluginException {
      // No native side (Android, tests): nothing to protect, always available.
      return true;
    }
  }

  // A phone without the native side (Android, tests) just has no extra time.
  @override
  Future<void> begin() async {
    try {
      await _channel.invokeMethod<void>('begin');
    } on MissingPluginException {
      // Nothing to ask for.
    }
  }

  @override
  Future<void> end() async {
    try {
      await _channel.invokeMethod<void>('end');
    } on MissingPluginException {
      // Nothing to give back.
    }
  }
}
