import 'dart:async';

import 'package:vyajena/app/background_task.dart';

/// The phone's background time, recorded for widget tests.
class FakeBackgroundTask implements BackgroundTaskApi {
  /// `begin` and `end`, in order.
  final calls = <String>[];
  final _expired = StreamController<void>.broadcast();

  /// The system says the time is over.
  void expire() => _expired.add(null);

  @override
  Stream<void> get expired => _expired.stream;

  @override
  Future<void> begin() async => calls.add('begin');

  @override
  Future<void> end() async => calls.add('end');
}
