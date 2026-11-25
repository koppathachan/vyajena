// app-ui: Create a sticker. Where a subject cutout comes from: the platform's
// own on-device segmentation (iOS Vision, Android ML Kit), behind one small
// interface. The native side only produces the straight-alpha RGBA cutout
// (photo-sticker: Segmentation seam and RGBA contract); the core trims, encodes
// WebP-with-alpha and sends it (photo-sticker: Sticker styling and encode), so
// nothing here encodes, encrypts or uploads. No model is bundled or downloaded
// and the photo never leaves the device.
//
// This is the one file that talks to the native segmentation plugin; widget
// tests give the screens a fake (test/support/fake_sticker.dart) through
// [SegmenterScope].
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core/core_service.dart';

/// A subject cut out of a photo on the device: a straight-alpha, 8-bit RGBA
/// image (sRGB, R,G,B,A byte order, row stride [width]×4, orientation applied),
/// with background pixels fully transparent. This is the only shape that
/// crosses to the core (photo-sticker: Segmentation seam and RGBA contract).
class Cutout {
  const Cutout({
    required this.source,
    required this.width,
    required this.height,
    required this.pixels,
  });

  /// Which on-device engine produced it.
  final StickerSource source;

  /// Pixel width of the RGBA image.
  final int width;

  /// Pixel height of the RGBA image.
  final int height;

  /// The RGBA bytes: [width]×[height]×4, R,G,B,A.
  final Uint8List pixels;
}

/// Why a cutout could not be made, so the app can tell the user (app-ui:
/// Create a sticker; "No subject", "Not supported").
enum SegmentError {
  /// The engine found no clear subject in the photo.
  noSubject,

  /// This platform has no on-device subject segmentation (too old an OS).
  unsupported,
}

/// Thrown by a [Segmenter] when it cannot produce a cutout.
class SegmentException implements Exception {
  const SegmentException(this.reason);
  final SegmentError reason;
}

/// The platform's on-device subject segmentation.
abstract class Segmenter {
  const Segmenter();

  /// Whether this platform can cut a subject out at all (the OS has the API);
  /// the sticker entry point is hidden when it cannot.
  Future<bool> available();

  /// Cuts the subject out of [photo] (the picked file's bytes) on the device.
  /// Throws [SegmentException] with [SegmentError.noSubject] when the engine
  /// finds no subject, or [SegmentError.unsupported] when the OS lacks the API.
  Future<Cutout> cutout(Uint8List photo);
}

/// The platform segmenter over a method channel to native code: iOS Vision
/// (`VNGenerateForegroundInstanceMaskRequest`) and Android ML Kit Subject
/// Segmentation. The native side returns the RGBA cutout and nothing else.
class DeviceSegmenter extends Segmenter {
  const DeviceSegmenter();

  static const _channel = MethodChannel('vyajena/sticker');

  @override
  Future<bool> available() async {
    try {
      return await _channel.invokeMethod<bool>('available') ?? false;
    } on PlatformException {
      return false;
    }
  }

  @override
  Future<Cutout> cutout(Uint8List photo) async {
    final Map<Object?, Object?>? result;
    try {
      result = await _channel.invokeMethod<Map<Object?, Object?>>('cutout', {
        'photo': photo,
      });
    } on PlatformException catch (error) {
      // The native side signals the two expected outcomes by error code.
      throw SegmentException(
        error.code == 'unsupported'
            ? SegmentError.unsupported
            : SegmentError.noSubject,
      );
    }
    if (result == null) throw const SegmentException(SegmentError.noSubject);
    final rgba = result['rgba'] as Uint8List?;
    final width = (result['width'] as num?)?.toInt();
    final height = (result['height'] as num?)?.toInt();
    if (rgba == null || width == null || height == null) {
      throw const SegmentException(SegmentError.noSubject);
    }
    return Cutout(
      source: _platformSource,
      width: width,
      height: height,
      pixels: rgba,
    );
  }

  /// Which engine this build's native plugin uses.
  static StickerSource get _platformSource =>
      defaultTargetPlatform == TargetPlatform.iOS
      ? StickerSource.iosVision
      : StickerSource.androidMlKit;
}

/// Offers a [Segmenter] to every screen below it, so a widget test can give the
/// app a fake without threading it through the screens.
class SegmenterScope extends InheritedWidget {
  const SegmenterScope({
    super.key,
    required this.segmenter,
    required super.child,
  });

  final Segmenter segmenter;

  /// The segmenter in scope; the platform's when there is none.
  static Segmenter of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<SegmenterScope>()?.segmenter ??
      const DeviceSegmenter();

  @override
  bool updateShouldNotify(SegmenterScope old) => segmenter != old.segmenter;
}
