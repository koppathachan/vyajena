// app-ui: Arcade motion. A stepped curve: the value jumps in [steps] equal
// steps, like CSS `steps(n)`. No animation package.
import 'package:flutter/widgets.dart';

class Steps extends Curve {
  const Steps(this.steps) : assert(steps > 0);

  final int steps;

  @override
  double transformInternal(double t) => (t * steps).floorToDouble() / steps;
}

/// Hover/press: 200ms in 4 steps.
const hoverDuration = Duration(milliseconds: 200);
const hoverCurve = Steps(4);

/// Entrance slide: 800ms in 8 steps, 120ms between siblings.
const entranceDuration = Duration(milliseconds: 800);
const entranceStagger = Duration(milliseconds: 120);
const entranceCurve = Steps(8);

/// Float: one 3s loop of whole-pixel frames (motion.dart, floatFrames).
const floatDuration = Duration(seconds: 3);
