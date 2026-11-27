// A fake [VideoSource] for widget tests: records what was asked and returns a
// clip held in memory, so a test never touches the camera, the gallery or the
// native normalize plugin.
import 'dart:typed_data';

import 'package:vyajena/chat/video_plugins.dart';

class FakeVideoSource implements VideoSource {
  /// By default [record]/[pick] return a sample clip. Pass `backsOut: true` for
  /// a source where the user chose nothing (both return null).
  FakeVideoSource({NormalizedVideo? clip, bool backsOut = false})
    : _clip = backsOut ? null : (clip ?? _sample());

  /// The clip returned by [record] and [pick]; null means the user backed out.
  final NormalizedVideo? _clip;

  /// How many times each entry point was called.
  int recordCalls = 0;
  int pickCalls = 0;
  int multiCalls = 0;

  /// What a multi-pick returns; when null, [pickMultiple] falls back to [pick].
  List<NormalizedVideo>? many;

  @override
  Future<NormalizedVideo?> record() async {
    recordCalls++;
    return _clip;
  }

  @override
  Future<NormalizedVideo?> pick() async {
    pickCalls++;
    return _clip;
  }

  @override
  Future<List<NormalizedVideo>> pickMultiple() async {
    multiCalls++;
    final picked = many;
    if (picked != null) {
      many = null;
      return picked;
    }
    // FakeVideoSource implements (not extends) VideoSource, so the interface's
    // default is replicated here: a single pick as a one-item list.
    final one = await pick();
    return one == null ? const [] : [one];
  }

  static NormalizedVideo _sample() => NormalizedVideo(
    mp4: Uint8List.fromList(
      // An `ftyp` box so it looks like an MP4, then a little payload.
      [0, 0, 0, 0x18, ...'ftyp'.codeUnits, ...'isom'.codeUnits, 1, 2, 3, 4],
    ),
    durationMs: 12000,
    width: 854,
    height: 480,
    firstFrameRgba: Uint8List.fromList(List<int>.filled(16 * 16 * 4, 128)),
    frameWidth: 16,
    frameHeight: 16,
  );
}
