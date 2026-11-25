// A Segmenter for widget tests (app-ui: Create a sticker): no native code. A
// test sets what the next cutout yields — a [Cutout], no subject, or an
// unsupported OS.
import 'dart:typed_data';

import 'package:vyajena/chat/sticker_plugins.dart';
import 'package:vyajena/core/core_service.dart';

/// The platform segmentation, faked. By default it cuts out a small opaque
/// square on a transparent canvas; a test can make it find no subject, be
/// unavailable, or throw.
class FakeSegmenter extends Segmenter {
  FakeSegmenter({this.supported = true, this.result = Outcome.cutout});

  /// What [available] returns.
  bool supported;

  /// What the next [cutout] does.
  Outcome result;

  /// How many times [cutout] was asked.
  int cutouts = 0;

  @override
  Future<bool> available() async => supported;

  @override
  Future<Cutout> cutout(Uint8List photo) async {
    cutouts++;
    switch (result) {
      case Outcome.cutout:
        return Cutout(
          source: StickerSource.iosVision,
          width: 4,
          height: 4,
          pixels: _square(4),
        );
      case Outcome.noSubject:
        throw const SegmentException(SegmentError.noSubject);
      case Outcome.unsupported:
        throw const SegmentException(SegmentError.unsupported);
    }
  }

  /// A tiny straight-alpha RGBA: an opaque dot in a transparent canvas.
  static Uint8List _square(int edge) {
    final pixels = Uint8List(edge * edge * 4);
    for (var y = 1; y < edge - 1; y++) {
      for (var x = 1; x < edge - 1; x++) {
        final i = (y * edge + x) * 4;
        pixels[i] = 220;
        pixels[i + 1] = 60;
        pixels[i + 2] = 120;
        pixels[i + 3] = 255;
      }
    }
    return pixels;
  }
}

/// What the fake segmenter does next.
enum Outcome { cutout, noSubject, unsupported }
