import 'dart:async';

import 'package:vyajena/app/background_task.dart';

/// The phone's background time, recorded for widget tests.
class FakeBackgroundTask implements BackgroundTaskApi {
  /// `begin` and `end`, in order.
  final calls = <String>[];
  final _expired = StreamController<void>.broadcast();
  final _locked = StreamController<void>.broadcast();

  /// Whether protected data is available (the device is unlocked). Set false to
  /// act like a phone that is already locked.
  bool available = true;

  /// The system says the time is over.
  void expire() => _expired.add(null);

  /// The device locked: protected data became unavailable.
  void lock() {
    available = false;
    _locked.add(null);
  }

  @override
  Stream<void> get expired => _expired.stream;

  @override
  Stream<void> get locked => _locked.stream;

  @override
  Future<bool> protectedDataAvailable() async => available;

  @override
  Future<void> begin() async => calls.add('begin');

  @override
  Future<void> end() async => calls.add('end');
}
