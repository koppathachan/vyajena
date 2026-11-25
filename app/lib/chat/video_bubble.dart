// app-ui: Video bubble and player. A video message is drawn at the clip's
// aspect ratio (width/height): first the first-frame ThumbHash blur (decoded by
// the core) with a play overlay and the duration, then, once the user taps it,
// the clip is downloaded through the core (nothing downloads by itself) and
// played full screen; one video plays at a time. When the server no longer has
// the clip the bubble says "Video expired" and downloads nothing (the sender can
// send it again from the message's long-press menu); a failed download can be
// tried again by tapping.
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:video_player/video_player.dart';

import '../app/errors.dart';
import '../core/core_service.dart';
import 'audio_bubble.dart' show clipLengthText;
import 'image_bubble.dart' show sizeText;

/// What a video that is gone from the server says.
const videoExpiredText = 'Video expired';

/// What a download that failed says.
const videoFailedText = "Couldn't load the video. Tap to try again.";

/// The widest and tallest a clip's bubble is drawn in the chat.
const _maxWidth = 260.0;
const _maxHeight = 320.0;

/// The shortest long edge a bubble is drawn at, so the controls and notices fit.
const _minLongEdge = 140.0;

/// One video message: its blur, play affordance and duration, and a tap to
/// download then play.
class VideoBubble extends StatefulWidget {
  const VideoBubble({
    super.key,
    required this.core,
    required this.messageId,
    required this.video,
    this.openViewer,
  });

  final CoreService core;

  /// The message's local id, which names the clip to the core.
  final int messageId;

  final VideoView video;

  /// Opens the full-screen player on the clip at [path]. Defaults to a
  /// [VideoViewer] route; a test injects its own to avoid the platform plugin.
  final Future<void> Function(BuildContext context, String path)? openViewer;

  @override
  State<VideoBubble> createState() => _VideoBubbleState();
}

class _VideoBubbleState extends State<VideoBubble> {
  late final _blur = widget.core.thumbhashPreview(
    Uint8List.fromList(widget.video.thumbhash),
  );
  bool _loading = false;
  bool _expired = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    _expired = widget.video.state == MediaStatus.expired;
  }

  @override
  void didUpdateWidget(VideoBubble old) {
    super.didUpdateWidget(old);
    if (widget.video.state == MediaStatus.expired) _expired = true;
  }

  Future<void> _tap() async {
    if (_loading || _expired) return;
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      // The clip comes from the cache if it is there, else is downloaded; the
      // core hands back a file path, never the bytes.
      final path = await widget.core.downloadVideo(widget.messageId);
      if (!mounted) return;
      final open = widget.openViewer ?? _defaultOpen;
      await open(context, path);
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

  Future<void> _defaultOpen(BuildContext context, String path) =>
      Navigator.of(context).push(
        MaterialPageRoute<void>(builder: (_) => VideoViewer(path: path)),
      );

  /// The size the bubble is drawn at: the clip's own shape, within the limits.
  Size _size() {
    final w = widget.video.width.toDouble().clamp(1.0, double.infinity);
    final h = widget.video.height.toDouble().clamp(1.0, double.infinity);
    final longEdge = w > h ? w : h;
    final scale = [
      1.0 < _minLongEdge / longEdge ? _minLongEdge / longEdge : 1.0,
      _maxWidth / w,
      _maxHeight / h,
    ].reduce((a, b) => a < b ? a : b);
    return Size(w * scale, h * scale);
  }

  Widget _backdrop(BuildContext context) {
    final blur = _blur;
    if (blur != null) {
      return Image.memory(
        blur,
        key: const Key('video-blur'),
        fit: BoxFit.cover,
        filterQuality: FilterQuality.medium,
        gaplessPlayback: true,
      );
    }
    return ColoredBox(
      key: const Key('video-blur'),
      color: Theme.of(context).colorScheme.surfaceContainerHighest,
    );
  }

  Widget _overlay(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final text = Theme.of(context).textTheme;
    final onScrim = text.labelMedium?.copyWith(color: Colors.white);
    if (_loading) {
      return const Center(
        key: Key('video-loading'),
        child: CircularProgressIndicator(),
      );
    }
    if (_expired) {
      return Align(
        alignment: Alignment.bottomCenter,
        child: Container(
          width: double.infinity,
          color: scheme.scrim.withValues(alpha: 0.55),
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
          child: Text(
            videoExpiredText,
            key: const Key('video-expired'),
            textAlign: TextAlign.center,
            style: onScrim,
          ),
        ),
      );
    }
    if (_error case final error?) {
      return Align(
        alignment: Alignment.bottomCenter,
        child: Container(
          width: double.infinity,
          color: scheme.scrim.withValues(alpha: 0.55),
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(videoFailedText, key: const Key('video-failed'), style: onScrim),
              Text(error, style: onScrim, textAlign: TextAlign.center),
            ],
          ),
        ),
      );
    }
    // The play control and, under it, the duration and size.
    final length = clipLengthText(
      Duration(milliseconds: widget.video.durationMs),
    );
    return Stack(
      children: [
        Center(
          child: Container(
            key: const Key('video-play'),
            padding: const EdgeInsets.all(10),
            decoration: BoxDecoration(
              color: scheme.scrim.withValues(alpha: 0.55),
              shape: BoxShape.circle,
            ),
            child: const Icon(Icons.play_arrow, color: Colors.white, size: 30),
          ),
        ),
        Align(
          alignment: Alignment.bottomRight,
          child: Container(
            margin: const EdgeInsets.all(6),
            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
            decoration: BoxDecoration(
              color: scheme.scrim.withValues(alpha: 0.55),
              borderRadius: BorderRadius.circular(4),
            ),
            child: Text(
              '$length · ${sizeText(widget.video.size)}',
              key: const Key('video-length'),
              style: onScrim,
            ),
          ),
        ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final size = _size();
    return Semantics(
      label: 'Video, tap to play',
      button: true,
      child: InkWell(
        key: const Key('video-bubble'),
        onTap: _tap,
        child: ClipRRect(
          borderRadius: BorderRadius.circular(8),
          child: SizedBox(
            width: size.width,
            height: size.height,
            child: Stack(
              fit: StackFit.expand,
              children: [_backdrop(context), _overlay(context)],
            ),
          ),
        ),
      ),
    );
  }
}

/// A clip full screen with play/pause and a scrubber. Plays the file at [path]
/// (the core's cached MP4); the bytes and key stay in the core.
class VideoViewer extends StatefulWidget {
  const VideoViewer({super.key, required this.path});

  final String path;

  @override
  State<VideoViewer> createState() => _VideoViewerState();
}

class _VideoViewerState extends State<VideoViewer> {
  late final VideoPlayerController _controller;
  bool _ready = false;

  @override
  void initState() {
    super.initState();
    _controller = VideoPlayerController.file(File(widget.path))
      ..initialize().then((_) {
        if (!mounted) return;
        setState(() => _ready = true);
        _controller.play();
      });
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    key: const Key('video-viewer'),
    backgroundColor: Colors.black,
    appBar: AppBar(
      backgroundColor: Colors.black,
      foregroundColor: Colors.white,
      elevation: 0,
    ),
    body: Center(
      child: _ready
          ? AspectRatio(
              aspectRatio: _controller.value.aspectRatio,
              child: Stack(
                alignment: Alignment.bottomCenter,
                children: [
                  VideoPlayer(_controller),
                  VideoProgressIndicator(_controller, allowScrubbing: true),
                  Center(
                    child: IconButton(
                      key: const Key('video-viewer-toggle'),
                      iconSize: 56,
                      color: Colors.white,
                      icon: Icon(
                        _controller.value.isPlaying
                            ? Icons.pause_circle
                            : Icons.play_circle,
                      ),
                      onPressed: () => setState(
                        () => _controller.value.isPlaying
                            ? _controller.pause()
                            : _controller.play(),
                      ),
                    ),
                  ),
                ],
              ),
            )
          : const CircularProgressIndicator(color: Colors.white),
    ),
  );
}
