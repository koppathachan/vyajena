// app-ui: Send an image. Where a picture to send comes from: the phone's
// gallery or the in-app camera. The picked file's bytes go to the core as they are; the core makes
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

  /// Captures a photo with the camera and returns its bytes, or null if the
  /// user backed out (app-ui: Send an image — capture or pick, matching the
  /// video record/gallery chooser). Defaults to [pick] so a gallery-only
  /// picker — and the widget-test fakes — need not override it.
  Future<Uint8List?> capture() => pick();

  /// Picks one or more pictures from the gallery (app-ui: Send an image —
  /// multiple from the gallery), each as its own message. Returns the chosen
  /// pictures' bytes in order, or empty if the user chose none. Defaults to a
  /// single [pick] so the test fakes need not override it.
  Future<List<Uint8List>> pickMultiple() async {
    final one = await pick();
    return one == null ? const [] : [one];
  }
}

/// The phone's gallery (iOS photo picker, Android photo picker) for [pick] —
/// no permission is asked, the system shows only what the user selects — and
/// the in-app camera for [capture].
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
  Future<Uint8List?> pick() => _fromSource(gallery.ImageSource.gallery);

  @override
  Future<Uint8List?> capture() => _fromSource(gallery.ImageSource.camera);

  @override
  Future<List<Uint8List>> pickMultiple() async {
    // The gallery's native multi-select; each picked image is downscaled by the
    // platform the same way a single pick is, then sent as its own message.
    final picked = await gallery.ImagePicker().pickMultiImage(
      maxWidth: _longestEdge,
      maxHeight: _longestEdge,
      imageQuality: _quality,
    );
    return Future.wait(picked.map((x) => x.readAsBytes()));
  }

  /// Reads a picture from [source] — the gallery or the in-app camera — asking
  /// the platform for an already-downscaled JPEG. The camera needs
  /// `NSCameraUsageDescription` (iOS); on Android `image_picker` hands off to
  /// the system camera, so no `CAMERA` manifest permission is required.
  Future<Uint8List?> _fromSource(gallery.ImageSource source) async {
    // Asking for a size and quality makes the platform hand over a downscaled
    // JPEG, so a HEIC photo from an iPhone arrives as one the core can read and
    // the pick finishes fast instead of transcoding the full-resolution
    // original. The core still does the final scale, WebP and EXIF strip.
    final picked = await gallery.ImagePicker().pickImage(
      source: source,
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
