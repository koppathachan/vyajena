// app-ui: Send a video. Where a video comes from (the camera or the gallery)
// and how it is made ready for the core: the capture/pick plugin and the native
// normalize plugin, behind one small interface.
//
// video-messages: Video normalization in a native plugin (design D3). The core
// does crypto only; it cannot transcode H.264 on a phone. A recording is captured
// straight to H.264/MP4 bounded to 30 s; a gallery clip is handed to the native
// normalize plugin (AVFoundation on iOS, MediaCodec on Android) over a
// MethodChannel, which transcodes it to the 480p/~1 Mbps H.264/MP4 target, trims
// it to ≤ 30 s and extracts the first frame as RGBA. The result — the normalized
// MP4 bytes, its dimensions and duration, and the first frame — goes to the core,
// which encrypts and uploads it (CoreService.sendVideo). The plaintext MP4 lives
// only in the app's private temp until the core has it, then is removed.
//
// This is the one file that imports `image_picker` and talks to the native
// plugin; widget tests give the screens a fake (test/support/fake_video.dart)
// through [VideoScope].
import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:image_picker/image_picker.dart';

/// The 30-second cap the app records and trims to (protocol::video's
/// MAX_VIDEO_DURATION_MS). The server never sees duration; the app enforces it.
const videoMaxDuration = Duration(seconds: 30);

/// The 480p long-edge bound the native plugins normalize to (the Kotlin
/// `VideoNormalize.LONG_EDGE` / iOS `AVAssetExportPreset640x480`). A normalized
/// clip's reported dimensions keep the source aspect ratio scaled to fit this —
/// never cropped — so the long edge never exceeds it. Used to sanity-check the
/// plugin result below.
const videoLongEdge = 854;

/// A normalized clip ready for the core: canonical H.264/MP4 at the 480p/~1 Mbps
/// target, trimmed to ≤ 30 s, with its first frame for the ThumbHash. Widget
/// tests build one in memory.
class NormalizedVideo {
  const NormalizedVideo({
    required this.mp4,
    required this.durationMs,
    required this.width,
    required this.height,
    required this.firstFrameRgba,
    required this.frameWidth,
    required this.frameHeight,
  });

  /// The normalized MP4 bytes, which go to the core as they are.
  final Uint8List mp4;

  /// The clip's length in milliseconds (≤ 30 000).
  final int durationMs;

  /// The clip's pixel size.
  final int width;
  final int height;

  /// The first frame as RGBA, from which the core computes the ThumbHash; empty
  /// when the plugin could not extract one.
  final Uint8List firstFrameRgba;
  final int frameWidth;
  final int frameHeight;
}

/// Where a video comes from and how it is normalized, replaceable in tests.
abstract class VideoSource {
  /// Records a clip (H.264/MP4, bounded to 30 s at capture) and returns it
  /// normalized, or null if the user backed out.
  Future<NormalizedVideo?> record();

  /// Picks a clip from the gallery, trims it to ≤ 30 s and normalizes it, or
  /// null if the user backed out.
  Future<NormalizedVideo?> pick();

  /// Picks one or more clips from the gallery (app-ui: Send a video — multiple
  /// from the gallery), normalizing each; each goes as its own message. Returns
  /// the normalized clips in order, or empty if the user backed out. Defaults to
  /// a single [pick] so the test fakes need not override it.
  Future<List<NormalizedVideo>> pickMultiple() async {
    final one = await pick();
    return one == null ? const [] : [one];
  }
}

/// The phone's camera/gallery (`image_picker`) and the native normalize plugin.
class DeviceVideoSource implements VideoSource {
  const DeviceVideoSource();

  /// The MethodChannel to the native normalize plugin (AVFoundation /
  /// MediaCodec). `normalize` takes `{path, maxDurationMs}` and returns
  /// `{path, durationMs, width, height, frameRgba, frameWidth, frameHeight}`.
  static const _channel = MethodChannel('chat.vyajena/video');

  @override
  Future<NormalizedVideo?> record() => _fromPicker(ImageSource.camera);

  @override
  Future<NormalizedVideo?> pick() => _fromPicker(ImageSource.gallery);

  @override
  Future<List<NormalizedVideo>> pickMultiple() async {
    // `image_picker` has no multi-video picker, so this uses the multi-media
    // picker and keeps only the video files (any image picked here is dropped;
    // images go through the Photo attachment). Each kept clip is normalized.
    final picked = await ImagePicker().pickMultipleMedia();
    final clips = <NormalizedVideo>[];
    for (final file in picked) {
      if (!_isVideoPath(file.path)) continue;
      final clip = await normalize(file.path);
      if (clip != null) clips.add(clip);
    }
    return clips;
  }

  /// Whether [path] names a video file the normalize plugin can take, by its
  /// extension (the multi-media picker returns images and videos together).
  static bool _isVideoPath(String path) {
    const exts = {'mp4', 'mov', 'm4v', '3gp', 'webm', 'mkv', 'avi'};
    final dot = path.lastIndexOf('.');
    if (dot < 0) return false;
    return exts.contains(path.substring(dot + 1).toLowerCase());
  }

  Future<NormalizedVideo?> _fromPicker(ImageSource source) async {
    final picked = await ImagePicker().pickVideo(
      source: source,
      // The camera bounds the recording at capture; a gallery clip is trimmed by
      // the native plugin.
      maxDuration: source == ImageSource.camera ? videoMaxDuration : null,
    );
    if (picked == null) return null;
    return normalize(picked.path);
  }

  /// Runs the native normalize plugin on the clip at [inputPath] and reads the
  /// result. The native side writes the normalized MP4 to a private temp file
  /// and returns its path; this reads the bytes and deletes both temp files once
  /// the core has them (the plaintext never lingers in the clear).
  Future<NormalizedVideo?> normalize(String inputPath) async {
    final result = await _channel.invokeMapMethod<String, Object?>('normalize', {
      'path': inputPath,
      'maxDurationMs': videoMaxDuration.inMilliseconds,
    });
    if (result == null) return null;
    final outPath = result['path'] as String;
    final out = File(outPath);
    try {
      final mp4 = await out.readAsBytes();
      final frame = result['frameRgba'] as Uint8List? ?? Uint8List(0);
      final width = (result['width'] as num).toInt();
      final height = (result['height'] as num).toInt();
      // The plugin must report real, aspect-preserving display dimensions: both
      // positive, with the long edge scaled to fit [videoLongEdge] (never
      // cropped or stretched past it). A zero or over-bound value means the
      // native transcode mis-sized the clip (the Android crop class of bug).
      assert(
        width > 0 && height > 0,
        'normalized video has non-positive dimensions ${width}x$height',
      );
      assert(
        width <= videoLongEdge && height <= videoLongEdge,
        'normalized video long edge ${width}x$height exceeds $videoLongEdge',
      );
      return NormalizedVideo(
        mp4: mp4,
        durationMs: (result['durationMs'] as num).toInt(),
        width: width,
        height: height,
        firstFrameRgba: frame,
        frameWidth: (result['frameWidth'] as num?)?.toInt() ?? 0,
        frameHeight: (result['frameHeight'] as num?)?.toInt() ?? 0,
      );
    } finally {
      // The normalized plaintext is removed once its bytes are in hand; the
      // picked original is the OS's to clean up.
      unawaited(_delete(outPath));
      unawaited(_delete(inputPath));
    }
  }
}

Future<void> _delete(String path) async {
  try {
    await File(path).delete();
  } on FileSystemException {
    // Already gone.
  }
}

/// Offers a [VideoSource] to every screen below it, so a widget test can give
/// the app a fake without threading it through the screens.
class VideoScope extends InheritedWidget {
  const VideoScope({super.key, required this.source, required super.child});

  final VideoSource source;

  /// The source in scope; the phone's when there is none.
  static VideoSource of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<VideoScope>()?.source ??
      const DeviceVideoSource();

  @override
  bool updateShouldNotify(VideoScope old) => source != old.source;
}
