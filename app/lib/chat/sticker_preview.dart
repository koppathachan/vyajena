// app-ui: Create a sticker. The preview of a cutout before it is sent: the
// transparent RGBA drawn over a checkerboard so the cut-out edges and the
// transparency read. This only displays the pixels the native plugin produced;
// it does not transform them for the wire (the core encodes). A received
// sticker is drawn by the image bubble, not here.
import 'dart:ui' as ui;

import 'package:flutter/material.dart';

import 'sticker_plugins.dart';

/// Draws [cutout]'s RGBA over a checkerboard. Decoding the raw RGBA to a
/// drawable image is async, so a blank box shows for the first frame.
class StickerPreview extends StatefulWidget {
  const StickerPreview({super.key, required this.cutout, this.size = 120});

  final Cutout cutout;

  /// The box's side in logical pixels.
  final double size;

  @override
  State<StickerPreview> createState() => _StickerPreviewState();
}

class _StickerPreviewState extends State<StickerPreview> {
  ui.Image? _image;

  @override
  void initState() {
    super.initState();
    _decode();
  }

  @override
  void didUpdateWidget(StickerPreview old) {
    super.didUpdateWidget(old);
    if (old.cutout.pixels != widget.cutout.pixels) {
      _image?.dispose();
      _image = null;
      _decode();
    }
  }

  void _decode() {
    ui.decodeImageFromPixels(
      widget.cutout.pixels,
      widget.cutout.width,
      widget.cutout.height,
      ui.PixelFormat.rgba8888,
      (image) {
        if (!mounted) {
          image.dispose();
          return;
        }
        setState(() => _image = image);
      },
    );
  }

  @override
  void dispose() {
    _image?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final image = _image;
    return ClipRRect(
      borderRadius: BorderRadius.zero,
      child: SizedBox(
        width: widget.size,
        height: widget.size,
        child: CustomPaint(
          painter: _CheckerPainter(Theme.of(context).brightness),
          foregroundPainter: image == null ? null : _StickerPainter(image),
        ),
      ),
    );
  }
}

/// A grey checkerboard, the universal "this is transparent" backdrop.
class _CheckerPainter extends CustomPainter {
  _CheckerPainter(this.brightness);

  final Brightness brightness;
  static const _cell = 10.0;

  @override
  void paint(Canvas canvas, Size size) {
    final dark = brightness == Brightness.dark;
    final a = Paint()
      ..color = dark ? const Color(0xFF3A3A3A) : const Color(0xFFE8E8E8);
    final b = Paint()
      ..color = dark ? const Color(0xFF2E2E2E) : const Color(0xFFFFFFFF);
    for (var y = 0.0; y < size.height; y += _cell) {
      for (var x = 0.0; x < size.width; x += _cell) {
        final even = ((x ~/ _cell) + (y ~/ _cell)).isEven;
        canvas.drawRect(Rect.fromLTWH(x, y, _cell, _cell), even ? a : b);
      }
    }
  }

  @override
  bool shouldRepaint(_CheckerPainter old) => old.brightness != brightness;
}

/// The cutout, scaled to fit the box, keeping its aspect ratio.
class _StickerPainter extends CustomPainter {
  _StickerPainter(this.image);

  final ui.Image image;

  @override
  void paint(Canvas canvas, Size size) {
    final src = Rect.fromLTWH(
      0,
      0,
      image.width.toDouble(),
      image.height.toDouble(),
    );
    final scale = (size.width / image.width) < (size.height / image.height)
        ? size.width / image.width
        : size.height / image.height;
    final w = image.width * scale;
    final h = image.height * scale;
    final dst = Rect.fromLTWH(
      (size.width - w) / 2,
      (size.height - h) / 2,
      w,
      h,
    );
    canvas.drawImageRect(image, src, dst, Paint());
  }

  @override
  bool shouldRepaint(_StickerPainter old) => old.image != image;
}
