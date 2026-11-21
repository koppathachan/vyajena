// app-ui: Play a voice message. One clip plays at a time in a chat: starting
// another stops the first, and nothing starts by itself when one ends (no
// auto-advance, owner 2026-11-21). The chat screen owns one of these and stops
// it when the chat closes.
import 'dart:async';

import 'package:flutter/foundation.dart';

import 'voice_plugins.dart';

/// Which clip is playing and how far it has got. Bubbles listen to this and
/// draw from it.
class VoicePlayback extends ChangeNotifier {
  /// The player is made when the first clip plays, so a chat nobody plays
  /// anything in never touches the audio plugin.
  VoicePlayback(this._newPlayer);

  final VoicePlayer Function() _newPlayer;
  VoicePlayer? _made;
  List<StreamSubscription<Object?>> _subscriptions = [];
  int? _current;
  bool _playing = false;
  Duration _position = Duration.zero;
  bool _disposed = false;

  VoicePlayer get _player => _made ??= _listen(_newPlayer());

  VoicePlayer _listen(VoicePlayer player) {
    _subscriptions = [
      player.position.listen((position) {
        if (_current == null) return;
        _position = position;
        notifyListeners();
      }),
      player.playing.listen((playing) {
        if (_current == null || _playing == playing) return;
        _playing = playing;
        notifyListeners();
      }),
      player.completed.listen((_) {
        // The clip ended: back to the start, and nothing else begins.
        if (_current == null) return;
        _playing = false;
        _position = Duration.zero;
        notifyListeners();
      }),
    ];
    return player;
  }

  /// The message whose clip is loaded: playing or paused. Null when none is.
  int? get current => _current;

  /// Whether [id] is the clip coming out of the speaker now.
  bool isPlaying(int id) => _current == id && _playing;

  /// How far into [id] the player is; zero for any other clip.
  Duration positionOf(int id) => _current == id ? _position : Duration.zero;

  /// The play button of message [id] was tapped: it plays, pauses or resumes.
  /// Another clip that was playing stops first. [path] gives the file to play,
  /// downloading it if it must; it is asked for only when [id] is not already
  /// loaded.
  Future<void> toggle(int id, Future<String> Function() path) async {
    if (_current == id) {
      if (_playing) {
        await _player.pause();
      } else {
        // A clip that played to its end starts over.
        await _player.play();
      }
      return;
    }
    // Starting another clip stops the one playing.
    if (_current != null) {
      await _player.stop();
      _forget();
    }
    final file = await path();
    if (_disposed) return;
    _current = id;
    _position = Duration.zero;
    notifyListeners();
    try {
      await _player.open(file);
      if (_disposed || _current != id) return;
      await _player.play();
    } catch (_) {
      if (_current == id) _forget();
      rethrow;
    }
  }

  /// Stops whatever plays (the chat closed, or a recording began).
  Future<void> stop() async {
    if (_current == null) return;
    await _player.stop();
    _forget();
  }



  void _forget() {
    _current = null;
    _playing = false;
    _position = Duration.zero;
    if (!_disposed) notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    for (final subscription in _subscriptions) {
      unawaited(subscription.cancel());
    }
    final player = _made;
    if (player != null) unawaited(player.stop().then((_) => player.dispose()));
    super.dispose();
  }
}
