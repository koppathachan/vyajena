// app-ui: Arcade look and Hand-drawn lines. A square-ish box with a thick,
// hand-drawn outline and a hard offset shadow (blur 0). Its corners sit a
// couple of pixels off true (sketch.dart), from [seed] and the box's size, so
// boxes never line up exactly. Material's own elevation is blurred, so the
// shadow is painted here, outside the box only (never over its contents), by
// every card, button, dialog and bubble that takes this as its shape.
import 'package:flutter/material.dart';

import 'sketch.dart';
import 'tokens.dart';

class ArcadeBorder extends OutlinedBorder {
  const ArcadeBorder({
    required this.color,
    this.width = arcadeBorderWidth,
    this.shadow = 0,
    this.shadowColor = Colors.black,
    this.seed = 0,
    this.rough = sketchCorner,
  }) : super(side: BorderSide.none);

  final Color color;
  final double width;

  /// The shadow's offset (right and down); 0 for none.
  final double shadow;
  final Color shadowColor;

  /// Where this box's wobble comes from (mixed with its size).
  final int seed;

  /// How far the corners sit off true; 0 for a ruled box (the theme's).
  final double rough;

  @override
  EdgeInsetsGeometry get dimensions => EdgeInsets.all(width);

  @override
  Path getInnerPath(Rect rect, {TextDirection? textDirection}) =>
      sketchOutline(rect.deflate(width), seed, corner: rough);

  @override
  Path getOuterPath(Rect rect, {TextDirection? textDirection}) =>
      sketchOutline(rect, seed, corner: rough);

  @override
  void paint(Canvas canvas, Rect rect, {TextDirection? textDirection}) {
    if (shadow > 0) {
      // Only the part of the shifted box outside the box itself.
      final box = sketchOutline(rect, seed, corner: rough);
      final outside = Path.combine(
        PathOperation.difference,
        box.shift(Offset(shadow, shadow)),
        box,
      );
      canvas.drawPath(outside, Paint()..color = shadowColor);
    }
    paintSketchBox(
      canvas,
      rect.deflate(width / 2),
      seed,
      color: color,
      width: width,
      corner: rough,
    );
  }

  @override
  ArcadeBorder copyWith({BorderSide? side}) => ArcadeBorder(
    color: side?.color ?? color,
    width: side?.width ?? width,
    shadow: shadow,
    shadowColor: shadowColor,
    seed: seed,
    rough: rough,
  );

  @override
  ArcadeBorder scale(double t) => ArcadeBorder(
    color: color,
    width: width * t,
    shadow: shadow * t,
    shadowColor: shadowColor,
    seed: seed,
    rough: rough,
  );

  @override
  bool operator ==(Object other) =>
      other is ArcadeBorder &&
      other.color == color &&
      other.width == width &&
      other.shadow == shadow &&
      other.shadowColor == shadowColor &&
      other.seed == seed &&
      other.rough == rough;

  @override
  int get hashCode =>
      Object.hash(color, width, shadow, shadowColor, seed, rough);
}

/// A text field's outline, hand-drawn like [ArcadeBorder]; the floating
/// label's gap is left open in the top edge.
class SketchInputBorder extends InputBorder {
  const SketchInputBorder({
    super.borderSide = const BorderSide(width: arcadeBorderWidth),
    this.seed = 0,
    this.rough = sketchCorner,
  });

  final int seed;

  /// How far the corners sit off true; 0 for a ruled outline.
  final double rough;

  @override
  bool get isOutline => true;

  @override
  SketchInputBorder copyWith({BorderSide? borderSide}) =>
      SketchInputBorder(
        borderSide: borderSide ?? this.borderSide,
        seed: seed,
        rough: rough,
      );

  @override
  EdgeInsetsGeometry get dimensions => EdgeInsets.all(borderSide.width);

  @override
  Path getInnerPath(Rect rect, {TextDirection? textDirection}) =>
      sketchOutline(rect.deflate(borderSide.width), seed, corner: rough);

  @override
  Path getOuterPath(Rect rect, {TextDirection? textDirection}) =>
      sketchOutline(rect, seed, corner: rough);

  @override
  void paint(
    Canvas canvas,
    Rect rect, {
    double? gapStart,
    double gapExtent = 0.0,
    double gapPercentage = 0.0,
    TextDirection? textDirection,
  }) {
    if (borderSide.style == BorderStyle.none || borderSide.width == 0) return;
    final box = rect.deflate(borderSide.width / 2);
    final c = sketchCorners(box, seed, corner: rough);
    final width = borderSide.width;
    final color = borderSide.color;
    if (gapStart == null || gapExtent <= 0 || gapPercentage == 0) {
      paintSketchBox(
        canvas,
        box,
        seed,
        color: color,
        width: width,
        corner: rough,
      );
      return;
    }
    // The label's gap: the top edge is two strokes, with the label between.
    const pad = 4.0;
    final extent = (gapExtent + pad * 2) * gapPercentage;
    final start = textDirection == TextDirection.rtl
        ? rect.left + gapStart + pad - extent
        : rect.left + gapStart - pad;
    final end = start + extent;
    final top = (c[0].dy + c[1].dy) / 2;
    paintSketchStroke(
      canvas,
      [Offset(end.clamp(c[0].dx, c[1].dx), top), c[1], c[2], c[3], c[0]],
      seed,
      color: color,
      width: width,
      rough: rough,
    );
    if (start > c[0].dx + 2) {
      paintSketchStroke(
        canvas,
        [c[0], Offset(start, top)],
        seed + 1,
        color: color,
        width: width,
        rough: rough,
      );
    }
  }

  @override
  SketchInputBorder scale(double t) =>
      SketchInputBorder(
        borderSide: borderSide.scale(t),
        seed: seed,
        rough: rough,
      );

  @override
  bool operator ==(Object other) =>
      other is SketchInputBorder &&
      other.borderSide == borderSide &&
      other.seed == seed &&
      other.rough == rough;

  @override
  int get hashCode => Object.hash(borderSide, seed, rough);
}

/// A plain rectangle with hand-drawn lines on some of its edges only: an app
/// bar's bottom edge, a navigation bar's top edge.
class SketchEdgeBorder extends ShapeBorder {
  const SketchEdgeBorder({
    required this.color,
    this.width = arcadeBorderWidth,
    this.top = false,
    this.bottom = false,
    this.left = false,
    this.seed = 0,
    this.rough = sketchCorner,
  });

  final Color color;
  final double width;
  final bool top;
  final bool bottom;
  final bool left;
  final int seed;

  /// 0 for a ruled edge.
  final double rough;

  @override
  EdgeInsetsGeometry get dimensions => EdgeInsets.only(
    top: top ? width : 0,
    bottom: bottom ? width : 0,
    left: left ? width : 0,
  );

  @override
  Path getInnerPath(Rect rect, {TextDirection? textDirection}) =>
      Path()..addRect(dimensions.resolve(textDirection).deflateRect(rect));

  @override
  Path getOuterPath(Rect rect, {TextDirection? textDirection}) =>
      Path()..addRect(rect);

  @override
  void paint(Canvas canvas, Rect rect, {TextDirection? textDirection}) {
    final w = rect.width.round();
    if (top) {
      final y = rect.top + width / 2;
      paintSketchStroke(
        canvas,
        [Offset(rect.left, y), Offset(rect.right, y)],
        seed * 31 + w,
        color: color,
        width: width,
        rough: rough,
      );
    }
    if (bottom) {
      final y = rect.bottom - width / 2;
      paintSketchStroke(
        canvas,
        [Offset(rect.left, y), Offset(rect.right, y)],
        seed * 31 + w + 7,
        color: color,
        width: width,
        rough: rough,
      );
    }
    if (left) {
      final x = rect.left + width / 2;
      paintSketchStroke(
        canvas,
        [Offset(x, rect.top), Offset(x, rect.bottom)],
        seed * 31 + rect.height.round() + 13,
        color: color,
        width: width,
        rough: rough,
      );
    }
  }

  @override
  ShapeBorder scale(double t) => SketchEdgeBorder(
    color: color,
    width: width * t,
    top: top,
    bottom: bottom,
    left: left,
    seed: seed,
    rough: rough,
  );

  @override
  bool operator ==(Object other) =>
      other is SketchEdgeBorder &&
      other.color == color &&
      other.width == width &&
      other.top == top &&
      other.bottom == bottom &&
      other.left == left &&
      other.seed == seed &&
      other.rough == rough;

  @override
  int get hashCode =>
      Object.hash(color, width, top, bottom, left, seed, rough);
}

/// A box decoration with the hand-drawn [ArcadeBorder]: [fill] inside, a
/// [border] outline of [width], optionally a hard [shadow].
ShapeDecoration sketchBox({
  required Color border,
  Color? fill,
  double width = arcadeBorderWidth,
  double shadow = 0,
  Color shadowColor = Colors.black,
  int seed = 0,
  double rough = sketchCorner,
}) => ShapeDecoration(
  color: fill,
  shape: ArcadeBorder(
    color: border,
    width: width,
    shadow: shadow,
    shadowColor: shadowColor,
    seed: seed,
    rough: rough,
  ),
);
