// app-ui: Arcade motion. Stepped entrance and float, both static under the
// system's reduce-motion setting. Never used in the message list.
import 'package:flutter/material.dart';

import 'sketch.dart';
import 'steps.dart';
import 'tokens.dart';

bool reducedMotion(BuildContext context) =>
    MediaQuery.maybeDisableAnimationsOf(context) ?? false;

/// What has already entered this run, so a row scrolled back into view (or a
/// list rebuilt by an event) does not slide in a second time.
final Set<Object> _entered = <Object>{};

/// Forgets what has entered (a test starts each run clean).
void resetArcadeEntrances() => _entered.clear();

/// Slides [child] up into place once, when first shown: 800ms in 10 steps,
/// after [index] times 120ms (capped at six siblings). With an [id] it slides
/// in only the first time that id is shown in this run.
class ArcadeEntrance extends StatefulWidget {
  const ArcadeEntrance({
    super.key,
    this.index = 0,
    this.id,
    required this.child,
  });

  final int index;
  final Object? id;
  final Widget child;

  @override
  State<ArcadeEntrance> createState() => _ArcadeEntranceState();
}

class _ArcadeEntranceState extends State<ArcadeEntrance>
    with SingleTickerProviderStateMixin {
  AnimationController? _controller;
  late final Animation<double> _t;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_controller != null || reducedMotion(context)) return;
    if (widget.id != null && !_entered.add(widget.id!)) return;
    final delay = entranceStagger * widget.index.clamp(0, 6);
    final total = entranceDuration + delay;
    final controller = AnimationController(vsync: this, duration: total);
    _t = CurvedAnimation(
      parent: controller,
      curve: Interval(
        delay.inMilliseconds / total.inMilliseconds,
        1,
        curve: entranceCurve,
      ),
    );
    _controller = controller..forward();
  }

  @override
  void dispose() {
    _controller?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final controller = _controller;
    if (controller == null) return widget.child;
    return AnimatedBuilder(
      animation: _t,
      child: widget.child,
      builder: (context, child) => Opacity(
        opacity: _t.value == 0 ? 0 : 1,
        child: FractionalTranslation(
          translation: Offset(0, 0.4 * (1 - _t.value)),
          child: child,
        ),
      ),
    );
  }
}

/// Bobs [child] up and down like a sprite: whole-pixel jumps at a few frames a
/// second (never a smooth glide), one 3s loop. [phase] (0..1) shifts where in
/// the loop it starts, so a column of bobbing icons never moves in step.
/// Static under reduce-motion, so a test that pumps until settled is not held
/// up.
class ArcadeFloat extends StatefulWidget {
  const ArcadeFloat({
    super.key,
    this.distance = 4,
    this.phase = 0,
    required this.child,
  });

  /// How high it bobs, in logical pixels.
  final double distance;

  /// Where in the loop it starts (0..1).
  final double phase;
  final Widget child;

  @override
  State<ArcadeFloat> createState() => _ArcadeFloatState();
}

/// A loop position for [id], so each icon bobs at its own moment.
double floatPhase(Object id) => (sketchSeed('$id') % 997) / 997;

/// The bob's frames, as fractions of its height: up, hold, down, hold.
const List<double> floatFrames = [0, 0, 0.5, 1, 1, 1, 0.5, 0];

/// Which [floatFrames] entry shows at loop position [t] (0..1) with [phase].
int floatFrameAt(double t, double phase) =>
    (((t + phase) % 1) * floatFrames.length).floor() % floatFrames.length;

class _ArcadeFloatState extends State<ArcadeFloat>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: floatDuration,
  )..addListener(_tick);

  late int _frame = floatFrameAt(0, widget.phase);

  /// Rebuilds only when the frame changes (a few times a second), not on
  /// every tick.
  void _tick() {
    final frame = floatFrameAt(_controller.value, widget.phase);
    if (frame != _frame) setState(() => _frame = frame);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    // Still under reduce-motion, and under a theme that is not lively.
    if (reducedMotion(context) || !context.arcade.lively) {
      _controller.stop();
    } else if (!_controller.isAnimating) {
      _controller.repeat();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final still = reducedMotion(context) || !context.arcade.lively;
    // Whole logical pixels only: a sprite jumps, it does not glide.
    final dy = still ? 0.0 : -(widget.distance * floatFrames[_frame]).round();
    return Transform.translate(
      offset: Offset(0, dy.toDouble()),
      child: widget.child,
    );
  }
}

/// [children] with each content widget (not spacers or dividers) wrapped in an
/// [ArcadeEntrance], staggered in order.
List<Widget> arcadeEntrances(List<Widget> children) {
  var i = 0;
  return [
    for (final child in children)
      if (child is SizedBox || child is Divider || child is SketchDivider)
        child
      else
        ArcadeEntrance(index: i++, child: child),
  ];
}
