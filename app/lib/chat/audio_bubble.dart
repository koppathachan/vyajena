// app-ui: Play a voice message. A voice message is a bubble with a play/pause
// button, the waveform from the message body (drawn before anything downloads,
// filled as the clip plays) and the duration. The first tap downloads the clip
// through the core (nothing downloads by itself), then plays it; one clip plays
// at a time. When the server no longer has the clip the bubble says "Audio
// expired" and downloads nothing; the sender can send it again from the message's
// long-press menu. A failed download can be tried again by tapping.
import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../core/core_service.dart';
import 'voice_playback.dart';

/// What a voice message that is gone from the server says.
const audioExpiredText = 'Audio expired';

/// What a download that failed says.
const audioFailedText = "Couldn't load the voice message. Tap to try again.";

/// The bubble's width: the waveform and the buttons around it.
const _waveformWidth = 160.0;
const _waveformHeight = 32.0;

/// A clip's length as "0:07", "1:05".
String clipLengthText(Duration length) {
  final seconds = length.inSeconds;
  return '${seconds ~/ 60}:${(seconds % 60).toString().padLeft(2, '0')}';
}

/// The play control and waveform of one voice message.
class AudioBubble extends StatefulWidget {
  const AudioBubble({
    super.key,
    required this.core,
    required this.messageId,
    required this.audio,
    required this.playback,
  });

  final CoreService core;

  /// The message's local id, which names the clip to the core.
  final int messageId;

  final AudioView audio;

  /// The chat's one player.
  final VoicePlayback playback;

  @override
  State<AudioBubble> createState() => _AudioBubbleState();
}

class _AudioBubbleState extends State<AudioBubble> {
  bool _loading = false;
  bool _expired = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    _expired = widget.audio.state == MediaStatus.expired;
  }

  @override
  void didUpdateWidget(AudioBubble old) {
    super.didUpdateWidget(old);
    if (widget.audio.state == MediaStatus.expired) _expired = true;
  }

  Future<void> _tap() async {
    if (_loading || _expired) return;
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      await widget.playback.toggle(
        widget.messageId,
        () => widget.core.downloadAudio(widget.messageId),
      );
    } catch (error) {
      if (!mounted) return;
      if (error is CoreError && error.kind == CoreErrorKind.mediaExpired) {
        setState(() => _expired = true);
      } else {
        setState(() => _error = errorText(error));
      }
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Duration get _total => Duration(milliseconds: widget.audio.durationMs);

  @override
  Widget build(BuildContext context) {
    if (_expired) return _expiredRow(context);
    return AnimatedBuilder(
      animation: widget.playback,
      builder: (context, _) => _player(context),
    );
  }

  Widget _expiredRow(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Row(
      key: const Key('audio-bubble'),
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(Icons.mic_off, color: scheme.onSurfaceVariant),
        const SizedBox(width: 8),
        Text(
          audioExpiredText,
          key: const Key('audio-expired'),
          style: TextStyle(
            fontStyle: FontStyle.italic,
            color: scheme.onSurfaceVariant,
          ),
        ),
      ],
    );
  }

  Widget _player(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final playback = widget.playback;
    final id = widget.messageId;
    final playing = playback.isPlaying(id);
    final position = playback.positionOf(id);
    final progress = _total.inMilliseconds == 0
        ? 0.0
        : (position.inMilliseconds / _total.inMilliseconds).clamp(0.0, 1.0);
    final error = _error;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          key: const Key('audio-bubble'),
          mainAxisSize: MainAxisSize.min,
          children: [
            _loading
                ? const SizedBox(
                    key: Key('audio-loading'),
                    width: 48,
                    height: 48,
                    child: Padding(
                      padding: EdgeInsets.all(12),
                      child: CircularProgressIndicator(strokeWidth: 2.5),
                    ),
                  )
                : IconButton(
                    key: const Key('audio-play'),
                    tooltip: playing ? 'Pause' : 'Play',
                    onPressed: _tap,
                    icon: Icon(playing ? Icons.pause : Icons.play_arrow),
                  ),
            SizedBox(
              key: const Key('audio-waveform'),
              width: _waveformWidth,
              height: _waveformHeight,
              child: CustomPaint(
                painter: _WaveformPainter(
                  peaks: widget.audio.waveform,
                  progress: progress,
                  played: scheme.primary,
                  unplayed: scheme.onSurfaceVariant.withValues(alpha: 0.4),
                ),
              ),
            ),
            const SizedBox(width: 8),
            Text(
              key: const Key('audio-length'),
              // Elapsed time while the clip is loaded, its length otherwise.
              clipLengthText(position > Duration.zero ? position : _total),
              style: Theme.of(context).textTheme.labelMedium,
            ),
          ],
        ),
        if (error != null)
          Padding(
            padding: const EdgeInsets.only(left: 12),
            child: Text(
              audioFailedText,
              key: const Key('audio-failed'),
              style: Theme.of(
                context,
              ).textTheme.labelSmall?.copyWith(color: scheme.error),
            ),
          ),
      ],
    );
  }
}

/// Bars for the peaks, the played part in one colour and the rest in another.
class _WaveformPainter extends CustomPainter {
  const _WaveformPainter({
    required this.peaks,
    required this.progress,
    required this.played,
    required this.unplayed,
  });

  final List<int> peaks;
  final double progress;
  final Color played;
  final Color unplayed;

  /// With no peaks to draw the bubble still shows a row of low bars.
  static const _flat = 32;

  @override
  void paint(Canvas canvas, Size size) {
    final count = peaks.isEmpty ? _flat : peaks.length;
    final slot = size.width / count;
    final bar = Paint()..strokeCap = StrokeCap.round;
    bar.strokeWidth = (slot * 0.6).clamp(1.0, 4.0);
    for (var i = 0; i < count; i++) {
      final peak = peaks.isEmpty ? 40 : peaks[i];
      // Never a bar too short to see.
      final height = (peak / 255 * size.height).clamp(3.0, size.height);
      final x = slot * (i + 0.5);
      bar.color = (i + 0.5) / count <= progress ? played : unplayed;
      canvas.drawLine(
        Offset(x, (size.height - height) / 2),
        Offset(x, (size.height + height) / 2),
        bar,
      );
    }
  }

  @override
  bool shouldRepaint(_WaveformPainter old) =>
      old.progress != progress ||
      old.peaks != peaks ||
      old.played != played ||
      old.unplayed != unplayed;
}
