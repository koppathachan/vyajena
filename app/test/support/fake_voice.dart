// The microphone and speaker for widget tests: no plugin, no audio. A test
// drives what the "recorder" produced and what the "player" is doing.
import 'dart:async';
import 'dart:io';
import 'dart:typed_data';

import 'package:vyajena/chat/voice_plugins.dart';

/// A recorder that writes a real file, so the app's read-and-delete of it is
/// exercised, and reports the length a test sets.
class FakeRecorder implements VoiceRecorder {
  FakeRecorder(this.plugins);

  final FakeVoicePlugins plugins;

  @override
  Future<bool> hasPermission() async {
    plugins.permissionAsked++;
    return plugins.permission;
  }

  @override
  Future<void> start() async {
    plugins.started++;
    plugins.file = File(
      '${Directory.systemTemp.path}/fake-voice-${DateTime.now().microsecondsSinceEpoch}.wav',
    )..writeAsBytesSync(plugins.contents);
    plugins.lastPath = plugins.file!.path;
  }

  @override
  Future<VoiceClip?> stop() async {
    plugins.stopped++;
    final file = plugins.file;
    plugins.file = null;
    if (file == null) return null;
    return VoiceClip(path: file.path, length: plugins.length);
  }

  @override
  Future<void> cancel() async {
    plugins.cancelled++;
    final file = plugins.file;
    plugins.file = null;
    if (file != null && file.existsSync()) file.deleteSync();
  }

  @override
  Future<void> dispose() async {}
}

/// A player that plays nothing and reports what a test tells it to.
class FakePlayer implements VoicePlayer {
  FakePlayer(this.plugins);

  final FakeVoicePlugins plugins;
  final _position = StreamController<Duration>.broadcast();
  final _playing = StreamController<bool>.broadcast();
  final _completed = StreamController<void>.broadcast();

  /// What this player has been asked to open, in order.
  final List<String> opened = [];
  bool isPlaying = false;

  @override
  Stream<Duration> get position => _position.stream;

  @override
  Stream<bool> get playing => _playing.stream;

  @override
  Stream<void> get completed => _completed.stream;

  @override
  Future<Duration?> open(String path) async {
    opened.add(path);
    plugins.opened.add(path);
    return null;
  }

  @override
  Future<void> play() async {
    isPlaying = true;
    plugins.plays++;
    _playing.add(true);
  }

  @override
  Future<void> pause() async {
    isPlaying = false;
    _playing.add(false);
  }

  @override
  Future<void> stop() async {
    isPlaying = false;
    plugins.stops++;
    _playing.add(false);
  }

  /// The playback position moved.
  void seekedTo(Duration position) => _position.add(position);

  /// The clip played to its end.
  void finish() {
    isPlaying = false;
    _playing.add(false);
    _completed.add(null);
  }

  @override
  Future<void> dispose() async {}
}

/// The fake microphone and speaker. One player is shared, so a test sees every
/// clip the app plays through it; the app's rule is that only one plays.
class FakeVoicePlugins extends VoicePlugins {
  /// Whether the user lets the app record.
  bool permission = true;

  /// What the next recording holds, and how long it says it is.
  Uint8List contents = Uint8List.fromList([1, 2, 3, 4]);
  Duration length = const Duration(seconds: 5);

  int permissionAsked = 0;
  int started = 0;
  int stopped = 0;
  int cancelled = 0;
  int plays = 0;
  int stops = 0;

  /// The recording in progress.
  File? file;

  /// The file the latest recording was written to, whether or not it is still
  /// there.
  String? lastPath;

  /// Every path any player was asked to open.
  final List<String> opened = [];

  final List<FakePlayer> players = [];

  @override
  VoiceRecorder newRecorder() => FakeRecorder(this);

  @override
  VoicePlayer newPlayer() {
    final player = FakePlayer(this);
    players.add(player);
    return player;
  }
}
