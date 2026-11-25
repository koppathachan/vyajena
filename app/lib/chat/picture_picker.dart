// app-ui: Send an image. Where a picture to send comes from: the phone's
// gallery. The picked file's bytes go to the core as they are; the core makes
// the WebP, strips the metadata and scales it (client-runtime: Image encode
// pipeline), so nothing here touches pixels. This is the one file that
// imports the gallery plugin; widget tests give the screens a fake picker.
//
// The picker asks the platform for a modest, already-downscaled JPEG rather
// than the full-resolution original, so a pick returns quickly and the preview
// appears at once (owner feedback: attaching felt slow). The heavy work — the
// final WebP encode, EXIF strip and the authoritative scale — still happens in
// the core on Send; the platform transcode here only coerces a HEIC original
// into something the core decodes and shrinks a huge original so the pick does
// not stall. It never encodes WebP, strips EXIF or uploads (app-ui: App does
// no image work).
import 'dart:typed_data';

import 'package:flutter/widgets.dart';
import 'package:image_picker/image_picker.dart' as gallery;

/// Gives the user a way to choose a picture.
abstract class PicturePicker {
  const PicturePicker();

  /// The chosen picture's file bytes, or null if the user chose none.
  Future<Uint8List?> pick();
}

/// The phone's gallery (iOS photo picker, Android photo picker): no
/// permission is asked, the system shows only what the user selects.
class GalleryPicker extends PicturePicker {
  const GalleryPicker();

  /// The longest edge asked of the platform. It matches the core's
  /// `MAX_IMAGE_EDGE` (2048): the platform's fast native downscale hands the
  /// core an already right-sized image, so the pick returns quickly, the
  /// preview draws at once, and the core's own scale is a no-op. A bigger cap
  /// only made the platform re-encode far more pixels before the pick returned.
  static const double _longestEdge = 2048;

  /// The JPEG quality asked of the platform. Modest, since the core re-encodes
  /// to WebP at ~80 on Send regardless; a lower quality keeps the platform
  /// transcode fast without a visible loss after the core's own encode.
  static const int _quality = 85;

  @override
  Future<Uint8List?> pick() async {
    // Asking for a size and quality makes the platform hand over a downscaled
    // JPEG, so a HEIC photo from an iPhone arrives as one the core can read and
    // the pick finishes fast instead of transcoding the full-resolution
    // original. The core still does the final scale, WebP and EXIF strip.
    final picked = await gallery.ImagePicker().pickImage(
      source: gallery.ImageSource.gallery,
      maxWidth: _longestEdge,
      maxHeight: _longestEdge,
      imageQuality: _quality,
    );
    return picked?.readAsBytes();
  }
}

/// Offers a [PicturePicker] to every screen below it, so a widget test can
/// give the app a fake one without threading it through the screens.
class PicturePickerScope extends InheritedWidget {
  const PicturePickerScope({
    super.key,
    required this.picker,
    required super.child,
  });

  final PicturePicker picker;

  /// The picker in scope; the gallery when there is none.
  static PicturePicker of(BuildContext context) =>
      context
          .dependOnInheritedWidgetOfExactType<PicturePickerScope>()
          ?.picker ??
      const GalleryPicker();

  @override
  bool updateShouldNotify(PicturePickerScope old) => picker != old.picker;
}
