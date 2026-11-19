// app-ui: Send an image. Where a picture to send comes from: the phone's
// gallery. The picked file's bytes go to the core as they are; the core makes
// the WebP, strips the metadata and scales it (client-runtime: Image encode
// pipeline), so nothing here touches pixels. This is the one file that
// imports the gallery plugin; widget tests give the screens a fake picker.
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

  /// The longest edge asked of the platform: a pick is capped here so a huge
  /// original does not fill memory before the core scales it to 2048.
  static const double _longestEdge = 4096;

  @override
  Future<Uint8List?> pick() async {
    // Asking for a size and quality makes the platform hand over a JPEG, so a
    // HEIC photo from an iPhone arrives as one the core can read.
    final picked = await gallery.ImagePicker().pickImage(
      source: gallery.ImageSource.gallery,
      maxWidth: _longestEdge,
      maxHeight: _longestEdge,
      imageQuality: 95,
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
