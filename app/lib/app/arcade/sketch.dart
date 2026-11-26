// app-ui: Hand-drawn lines (change arcade-restyle, owner review 2). Every box
// and line is drawn as if by hand: corners a couple of pixels off true, edges
// slightly bowed, each stroke drawn twice like a pen going over it. The wobble
// comes from a seed, so a box keeps its shape on every repaint; boxes of
// different sizes (or seeds) get different shapes, so nothing lines up.
import 'dart:math' as math;

import 'package:flutter/material.dart';

/// How far a corner may sit off true.
const double sketchCorner = 2;

/// How far an edge may bow sideways.
const double sketchBow = 1.2;

/// A small deterministic random source (an LCG), so a seed always draws the
/// same wobble.
class SketchRandom {
  SketchRandom(int seed) : _s = seed & 0x7fffffff {
    next();
  }

  int _s;

  /// The next value in [0, 1).
  double next() {
    _s = (_s * 1103515245 + 12345) & 0x7fffffff;
    return _s / 0x80000000;
  }

  /// The next value in [-a, a].
  double range(double a) => (next() * 2 - 1) * a;
}

/// A stable seed for [text] (Dart's String.hashCode is stable within a run,
/// but this is stable across runs too).
int sketchSeed(String text) {
  var h = 0x811c9dc5;
  for (final c in text.codeUnits) {
    h = ((h ^ c) * 0x01000193) & 0x7fffffff;
  }
  return h;
}

/// [seed] mixed with [size], so two boxes with one seed but different sizes
/// still differ.
int _mix(int seed, Size size) =>
    (seed * 31 + size.width.round() * 131 + size.height.round() * 7919) &
    0x7fffffff;

/// The four corners of [rect] (top-left, top-right, bottom-right, bottom-left),
/// each moved by up to [corner] px.
List<Offset> sketchCorners(
  Rect rect,
  int seed, {
  double corner = sketchCorner,
}) {
  final r = SketchRandom(_mix(seed, rect.size));
  // Small boxes wobble less, so a 10px pill is not a rhombus.
  final c = math.min(corner, rect.shortestSide / 8);
  return [
    rect.topLeft + Offset(r.range(c), r.range(c)),
    rect.topRight + Offset(r.range(c), r.range(c)),
    rect.bottomRight + Offset(r.range(c), r.range(c)),
    rect.bottomLeft + Offset(r.range(c), r.range(c)),
  ];
}

/// The sideways unit normal of the segment [a]→[b].
Offset _normal(Offset a, Offset b) {
  final d = b - a;
  final len = d.distance;
  return len == 0 ? Offset.zero : Offset(-d.dy / len, d.dx / len);
}

/// The closed, slightly bowed outline of [rect]: the shape a box fills and
/// clips to.
Path sketchOutline(Rect rect, int seed, {double corner = sketchCorner}) {
  if (rect.isEmpty) return Path()..addRect(rect);
  final p = sketchCorners(rect, seed, corner: corner);
  final r = SketchRandom(_mix(seed ^ 0x5bd1e995, rect.size));
  final bow = math.min(sketchBow, rect.shortestSide / 12);
  final path = Path()..moveTo(p[0].dx, p[0].dy);
  for (var i = 0; i < 4; i++) {
    final a = p[i];
    final b = p[(i + 1) % 4];
    final mid = Offset.lerp(a, b, 0.5)! + _normal(a, b) * r.range(bow);
    path.quadraticBezierTo(mid.dx, mid.dy, b.dx, b.dy);
  }
  return path..close();
}

/// Adds a hand-drawn stroke from [a] to [b] to [path]: a cubic whose two
/// control points are pushed sideways, with ends that overshoot or fall short
/// by up to [overshoot] px.
void addSketchLine(
  Path path,
  Offset a,
  Offset b,
  SketchRandom r, {
  double bow = sketchBow,
  double overshoot = 1.5,
}) {
  final d = b - a;
  final len = d.distance;
  if (len == 0) return;
  final along = d / len;
  final n = _normal(a, b);
  // Long lines bow a little more, as a hand does.
  final b2 = bow * (0.6 + math.min(len, 300) / 300);
  final start = a - along * r.range(overshoot) + n * r.range(0.6);
  final end = b + along * r.range(overshoot) + n * r.range(0.6);
  final c1 = Offset.lerp(start, end, 0.3 + r.range(0.08))! + n * r.range(b2);
  final c2 = Offset.lerp(start, end, 0.7 + r.range(0.08))! + n * r.range(b2);
  path
    ..moveTo(start.dx, start.dy)
    ..cubicTo(c1.dx, c1.dy, c2.dx, c2.dy, end.dx, end.dy);
}

/// Strokes a hand-drawn polyline through [points] (closed when [closed]) with
/// [color] at about [width]: two passes with different wobble, the first a
/// little thinner than [width], the second thinner still.
void paintSketchStroke(
  Canvas canvas,
  List<Offset> points,
  int seed, {
  required Color color,
  required double width,
  bool closed = false,
}) {
  if (width <= 0 || points.length < 2) return;
  final segments = closed ? points.length : points.length - 1;
  for (final (pass, w) in [(0, width * 0.9), (1, width * 0.55)]) {
    final r = SketchRandom(seed * 7 + pass * 104729 + 1);
    final path = Path();
    for (var i = 0; i < segments; i++) {
      addSketchLine(path, points[i], points[(i + 1) % points.length], r);
    }
    canvas.drawPath(
      path,
      Paint()
        ..color = color
        ..style = PaintingStyle.stroke
        ..strokeWidth = w
        ..strokeCap = StrokeCap.round
        ..strokeJoin = StrokeJoin.round,
    );
  }
}

/// Strokes the hand-drawn outline of [rect].
void paintSketchBox(
  Canvas canvas,
  Rect rect,
  int seed, {
  required Color color,
  required double width,
  double corner = sketchCorner,
}) => paintSketchStroke(
  canvas,
  sketchCorners(rect, seed, corner: corner),
  _mix(seed, rect.size),
  color: color,
  width: width,
  closed: true,
);

/// A hand-drawn horizontal rule, for where a [Divider] would go.
class SketchDivider extends StatelessWidget {
  const SketchDivider({
    super.key,
    this.seed = 0,
    this.color,
    this.thickness = 2,
    this.height = 12,
    this.indent = 0,
  });

  final int seed;
  final Color? color;
  final double thickness;
  final double height;
  final double indent;

  @override
  Widget build(BuildContext context) => SizedBox(
    height: height,
    width: double.infinity,
    child: CustomPaint(
      painter: _LinePainter(
        seed: seed,
        color: color ?? Theme.of(context).colorScheme.outline,
        width: thickness,
        indent: indent,
      ),
    ),
  );
}

class _LinePainter extends CustomPainter {
  _LinePainter({
    required this.seed,
    required this.color,
    required this.width,
    required this.indent,
  });

  final int seed;
  final Color color;
  final double width;
  final double indent;

  @override
  void paint(Canvas canvas, Size size) {
    final y = size.height / 2;
    paintSketchStroke(
      canvas,
      [Offset(indent, y), Offset(size.width - indent, y)],
      seed * 31 + size.width.round(),
      color: color,
      width: width,
    );
  }

  @override
  bool shouldRepaint(_LinePainter old) =>
      old.seed != seed ||
      old.color != color ||
      old.width != width ||
      old.indent != indent;
}

/// [child] set a little askew: turned by up to [degrees] and shifted by up to
/// [shift] px, both from [seed], so a row of tiles never lines up exactly.
class Askew extends StatelessWidget {
  const Askew({
    super.key,
    required this.seed,
    this.degrees = 2,
    this.shift = 1.5,
    required this.child,
  });

  final int seed;
  final double degrees;
  final double shift;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final r = SketchRandom(seed * 2654435761);
    final angle = r.range(degrees) * math.pi / 180;
    final offset = Offset(r.range(shift), r.range(shift));
    return Transform.translate(
      offset: offset,
      child: Transform.rotate(angle: angle, child: child),
    );
  }
}
