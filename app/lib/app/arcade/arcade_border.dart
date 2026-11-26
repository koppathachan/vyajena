// app-ui: Arcade look. A square border with a thick outline and a hard
// offset shadow (blur 0). Material's own elevation is blurred, so the shadow
// is painted here, outside the widget's box only (never over its contents),
// by every card, button, dialog and bubble that takes this as its shape.
import 'package:flutter/material.dart';

import 'tokens.dart';

class ArcadeBorder extends OutlinedBorder {
  const ArcadeBorder({
    required this.color,
    this.width = arcadeBorderWidth,
    this.shadow = 0,
    this.shadowColor = Colors.black,
  }) : super(side: BorderSide.none);

  final Color color;
  final double width;

  /// The shadow's offset (right and down); 0 for none.
  final double shadow;
  final Color shadowColor;

  @override
  EdgeInsetsGeometry get dimensions => EdgeInsets.all(width);

  @override
  Path getInnerPath(Rect rect, {TextDirection? textDirection}) =>
      Path()..addRect(rect.deflate(width));

  @override
  Path getOuterPath(Rect rect, {TextDirection? textDirection}) =>
      Path()..addRect(rect);

  @override
  void paint(Canvas canvas, Rect rect, {TextDirection? textDirection}) {
    if (shadow > 0) {
      // Only the part of the shifted box outside the box itself.
      final outside = Path.combine(
        PathOperation.difference,
        Path()..addRect(rect.shift(Offset(shadow, shadow))),
        Path()..addRect(rect),
      );
      canvas.drawPath(outside, Paint()..color = shadowColor);
    }
    if (width > 0) {
      canvas.drawRect(
        rect.deflate(width / 2),
        Paint()
          ..color = color
          ..style = PaintingStyle.stroke
          ..strokeWidth = width,
      );
    }
  }

  @override
  ArcadeBorder copyWith({BorderSide? side}) => ArcadeBorder(
    color: side?.color ?? color,
    width: side?.width ?? width,
    shadow: shadow,
    shadowColor: shadowColor,
  );

  @override
  ArcadeBorder scale(double t) => ArcadeBorder(
    color: color,
    width: width * t,
    shadow: shadow * t,
    shadowColor: shadowColor,
  );

  @override
  bool operator ==(Object other) =>
      other is ArcadeBorder &&
      other.color == color &&
      other.width == width &&
      other.shadow == shadow &&
      other.shadowColor == shadowColor;

  @override
  int get hashCode => Object.hash(color, width, shadow, shadowColor);
}
