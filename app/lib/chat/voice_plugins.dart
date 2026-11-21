// app-ui: Record a voice message; Play a voice message. Where a recording comes
// from (the microphone) and where a clip is played (the speaker): the two
// plugins, behind two small interfaces. The recorder writes a plain WAV file
// and hands the app its path; the core makes the OGG/Opus from it (audio-messages:
// Recording and encoding), so nothing here encodes audio. The player is handed
// a file path, which is either the recorder's WAV (the preview) or the core's
// cached OGG/Opus (a received clip), and plays it as it is.
//
// This is the one file that imports `record` and `just_audio`; widget tests give
// the screens fakes (test/support/fake_voice.dart) through [VoiceScope].
import 'dart:async';
import 'dart:io';

import 'package:flutter/widgets.dart';
import 'package:just_audio/just_audio.dart';
import 'package:record/record.dart';

/// A finished recording: a WAV file and how long it is.
class VoiceClip {
  const VoiceClip({required this.path, required this.length});

  /// The temporary WAV file the recorder wrote. The app deletes it once the
  /// clip is sent or thrown away.
  final String path;

  /// The recording's length.
  final Duration length;
}

/// The microphone.
abstract class VoiceRecorder {
  /// Whether the app may record, asking the user if it has not been asked yet.
  Future<bool> hasPermission();

  /// Starts recording to a temporary file.
  Future<void> start();

  /// Stops and returns what was recorded, or null if nothing was.
  Future<VoiceClip?> stop();

  /// Stops and throws the recording away.
  Future<void> cancel();

  /// Releases the microphone.
  Future<void> dispose();
}

/// Plays one file at a time.
abstract class VoicePlayer {
  /// Where playback has got to.
  Stream<Duration> get position;

  /// Whether sound is coming out.
  Stream<bool> get playing;

  /// Emits once each time the file plays to its end.
  Stream<void> get completed;

  /// Opens the file at [path]; its length, if the player knows it.
  Future<Duration?> open(String path);

  /// Plays from where it is; returns at once, not at the end.
  Future<void> play();

  /// Pauses, keeping the position.
  Future<void> pause();

  /// Stops and goes back to the start.
  Future<void> stop();

  /// Releases the player.
  Future<void> dispose();
}

/// What the screens use to record and play, replaceable in tests.
abstract class VoicePlugins {
  const VoicePlugins();

  VoiceRecorder newRecorder();

  VoicePlayer newPlayer();
}

/// The phone's microphone (`record`) and speaker (`just_audio`).
class DeviceVoicePlugins extends VoicePlugins {
  const DeviceVoicePlugins();

  @override
  VoiceRecorder newRecorder() => _DeviceRecorder();

  @override
  VoicePlayer newPlayer() => _DevicePlayer();
}

class _DeviceRecorder implements VoiceRecorder {
  final AudioRecorder _recorder = AudioRecorder();
  String? _path;
  DateTime? _started;

  @override
  Future<bool> hasPermission() => _recorder.hasPermission();

  @override
  Future<void> start() async {
    // WAV, 16-bit mono at the rate Opus works at: the core reads it as it is
    // (spike S1: `record` writes this on iOS and Android, where its own Opus
    // would come as a CAF file on iOS).
    final path =
        '${Directory.systemTemp.path}/voice-${DateTime.now().microsecondsSinceEpoch}.wav';
    _path = path;
    _started = DateTime.now();
    await _recorder.start(
      const RecordConfig(
        encoder: AudioEncoder.wav,
        sampleRate: 48000,
        numChannels: 1,
      ),
      path: path,
    );
  }

  @override
  Future<VoiceClip?> stop() async {
    final started = _started;
    final stopped = await _recorder.stop() ?? _path;
    _path = null;
    _started = null;
    if (stopped == null || started == null) return null;
    return VoiceClip(
      path: stopped,
      length: DateTime.now().difference(started),
    );
  }

  @override
  Future<void> cancel() async {
    final path = await _recorder.stop() ?? _path;
    _path = null;
    _started = null;
    if (path != null) await _delete(path);
  }

  @override
  Future<void> dispose() => _recorder.dispose();
}

class _DevicePlayer implements VoicePlayer {
  final AudioPlayer _player = AudioPlayer();

  @override
  Stream<Duration> get position => _player.positionStream;

  @override
  Stream<bool> get playing => _player.playingStream;

  @override
  Stream<void> get completed => _player.processingStateStream
      .where((state) => state == ProcessingState.completed);

  @override
  Future<Duration?> open(String path) => _player.setFilePath(path);

  @override
  Future<void> play() async {
    // A clip that played to its end starts over.
    if (_player.processingState == ProcessingState.completed) {
      await _player.seek(Duration.zero);
    }
    // `just_audio`'s play() returns at the end of the clip: not awaited.
    unawaited(_player.play());
  }

  @override
  Future<void> pause() => _player.pause();

  @override
  Future<void> stop() async {
    await _player.pause();
    await _player.seek(Duration.zero);
  }

  @override
  Future<void> dispose() => _player.dispose();
}

/// Deletes a temporary recording; a file already gone is fine.
Future<void> deleteRecording(String path) => _delete(path);

Future<void> _delete(String path) async {
  try {
    await File(path).delete();
  } on FileSystemException {
    // Already gone.
  }
}

/// Offers [VoicePlugins] to every screen below it, so a widget test can give the
/// app fakes without threading them through the screens.
class VoiceScope extends InheritedWidget {
  const VoiceScope({super.key, required this.plugins, required super.child});

  final VoicePlugins plugins;

  /// The plugins in scope; the phone's when there are none.
  static VoicePlugins of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<VoiceScope>()?.plugins ??
      const DeviceVoicePlugins();

  @override
  bool updateShouldNotify(VoiceScope old) => plugins != old.plugins;
}
