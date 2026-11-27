// app-ui: Hand-drawn lines, Irregular boxes and Arcade motion (change
// arcade-restyle, owner review 2).
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/app/arcade/arcade_border.dart';
import 'package:vyajena/app/arcade/motion.dart';
import 'package:vyajena/app/arcade/sketch.dart';
import 'package:vyajena/app/arcade/steps.dart';
import 'package:vyajena/app/theme.dart';
import 'package:vyajena/chat/composer.dart';

const _box = Rect.fromLTWH(0, 0, 200, 60);

void main() {
  // spec: app-ui/hand-drawn-lines
  test('a box is drawn off true by at most 2px, the same way every time', () {
    final corners = sketchCorners(_box, 7);
    final truth = [
      _box.topLeft,
      _box.topRight,
      _box.bottomRight,
      _box.bottomLeft,
    ];
    var off = 0.0;
    for (var i = 0; i < 4; i++) {
      final d = corners[i] - truth[i];
      expect(d.dx.abs(), lessThanOrEqualTo(sketchCorner));
      expect(d.dy.abs(), lessThanOrEqualTo(sketchCorner));
      off += d.distance;
    }
    expect(off, greaterThan(0.5), reason: 'not a ruled rectangle');
    // Stable: the same seed and size draw the same box on every repaint.
    expect(sketchCorners(_box, 7), corners);
    // Irregular: another seed, or another size, draws another box.
    expect(sketchCorners(_box, 8), isNot(corners));
    expect(
      sketchCorners(const Rect.fromLTWH(0, 0, 201, 60), 7),
      isNot(corners),
    );
  });

  // spec: app-ui/hand-drawn-lines
  test('the fill follows the hand-drawn outline, not the rectangle', () {
    const border = ArcadeBorder(color: Colors.black, seed: 3);
    final outline = border.getOuterPath(_box).getBounds();
    expect(outline, isNot(_box));
    expect(
      (outline.left - _box.left).abs(),
      lessThanOrEqualTo(sketchCorner + sketchBow),
    );
    expect(
      (outline.right - _box.right).abs(),
      lessThanOrEqualTo(sketchCorner + sketchBow),
    );
  });

  // spec: app-ui/hand-drawn-lines
  test('every theme line is hand drawn: fields, app bar, boxes', () {
    for (final theme in [lightTheme, darkTheme]) {
      expect(
        theme.inputDecorationTheme.enabledBorder,
        isA<SketchInputBorder>(),
      );
      expect(
        theme.inputDecorationTheme.focusedBorder,
        isA<SketchInputBorder>(),
      );
      expect(theme.appBarTheme.shape, isA<SketchEdgeBorder>());
      expect((theme.appBarTheme.shape! as SketchEdgeBorder).bottom, isTrue);
      expect(theme.cardTheme.shape, isA<ArcadeBorder>());
      expect(theme.tooltipTheme.decoration, isA<ShapeDecoration>());
    }
  });

  // spec: app-ui/hand-drawn-lines
  testWidgets('a field with a floating label paints without error', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: lightTheme,
        home: const Scaffold(
          body: Padding(
            padding: EdgeInsets.all(16),
            child: TextField(
              decoration: InputDecoration(labelText: 'Display name'),
            ),
          ),
        ),
      ),
    );
    await tester.enterText(find.byType(TextField), 'Alice');
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
  });

  // spec: app-ui/arcade-motion
  test('the bob jumps in whole pixels and icons keep their own beat', () {
    // Every frame is a whole number of pixels high for the 3px and 4px bobs.
    for (final distance in [3.0, 4.0]) {
      for (final f in floatFrames) {
        final dy = (distance * f).round().toDouble();
        expect(dy, dy.roundToDouble());
      }
    }
    // A few frames a second, not a smooth glide: at most 8 positions in 3s.
    expect(floatFrames.length, lessThanOrEqualTo(8));
    expect(
      floatDuration.inMilliseconds / floatFrames.length,
      greaterThanOrEqualTo(300),
    );
    // Two icons with different ids start at different moments of the loop.
    final phases = {
      for (final id in ['a', 'b', 'c', 'd']) floatPhase(id),
    };
    expect(phases.length, greaterThan(1));
    expect(floatFrameAt(0, 0), 0);
    expect(floatFrameAt(0.99, 0), floatFrames.length - 1);
  });

  // spec: app-ui/irregular-boxes
  testWidgets('dark mode: the send icon is navy on the yellow button', (
    tester,
  ) async {
    late ButtonStyle style;
    await tester.pumpWidget(
      MaterialApp(
        theme: darkTheme,
        home: Builder(
          builder: (context) {
            style = sendButtonStyle(context);
            return const SizedBox();
          },
        ),
      ),
    );
    final scheme = darkTheme.colorScheme;
    expect(style.backgroundColor!.resolve({}), scheme.primary);
    expect(style.foregroundColor!.resolve({}), scheme.onPrimary);
    expect(scheme.onPrimary, const Color(0xFF0F0F23));
    // Smaller than the field (at least 44px tall), so they do not line up.
    expect(style.fixedSize!.resolve({})!.height, lessThan(44));
  });
}
