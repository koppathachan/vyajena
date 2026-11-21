// The microphone and speaker for widget tests: no plugin, no audio. A test
// drives what the "recorder" produced and what the "player" is doing.
import 'dart:async';
import 'dart:typed_data';

import 'package:vyajena/chat/voice_plugins.dart';

/// A recording held in memory (a real file's I/O never finishes under a widget
/// test's fake clock), which remembers whether the app deleted it.
class FakeClip extends VoiceClip {
  FakeClip(this.bytes, Duration length)
    : super(path: '/tmp/fake-voice.wav', length: length);

  final Uint8List bytes;
  bool deleted = false;

  @override
  Future<Uint8List> read() async => bytes;

  @override
  Future<void> delete() async => deleted = true;
}

/// A recorder that reports the length a test sets.
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
    plugins.recording = FakeClip(plugins.contents, plugins.length);
    plugins.lastClip = plugins.recording;
  }

  @override
  Future<VoiceClip?> stop() async {
    plugins.stopped++;
    final clip = plugins.recording;
    plugins.recording = null;
    return clip;
  }

  @override
  Future<void> cancel() async {
    plugins.cancelled++;
    plugins.recording?.deleted = true;
    plugins.recording = null;
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
  FakeClip? recording;

  /// The latest recording, whether or not it is still in progress.
  FakeClip? lastClip;

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
