// app-ui: Arcade motion. Stepped entrance and float, both static under the
// system's reduce-motion setting. Never used in the message list.
import 'package:flutter/material.dart';

import 'steps.dart';

bool reducedMotion(BuildContext context) =>
    MediaQuery.maybeDisableAnimationsOf(context) ?? false;

/// What has already entered this run, so a row scrolled back into view (or a
/// list rebuilt by an event) does not slide in a second time.
final Set<Object> _entered = <Object>{};

/// Forgets what has entered (a test starts each run clean).
void resetArcadeEntrances() => _entered.clear();

/// Slides [child] up into place once, when first shown: 800ms in 8 steps,
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

/// Bobs [child] up and down: 3s in 5 steps, repeating. Static under
/// reduce-motion, so a test that pumps until settled is not held up.
class ArcadeFloat extends StatefulWidget {
  const ArcadeFloat({super.key, this.distance = 6, required this.child});

  final double distance;
  final Widget child;

  @override
  State<ArcadeFloat> createState() => _ArcadeFloatState();
}

class _ArcadeFloatState extends State<ArcadeFloat>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: floatDuration ~/ 2,
  );

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (reducedMotion(context)) {
      _controller.stop();
    } else if (!_controller.isAnimating) {
      _controller.repeat(reverse: true);
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: _controller,
    child: widget.child,
    builder: (context, child) => Transform.translate(
      offset: Offset(
        0,
        -widget.distance * floatCurve.transform(_controller.value),
      ),
      child: child,
    ),
  );
}

/// [children] with each content widget (not spacers or dividers) wrapped in an
/// [ArcadeEntrance], staggered in order.
List<Widget> arcadeEntrances(List<Widget> children) {
  var i = 0;
  return [
    for (final child in children)
      if (child is SizedBox || child is Divider)
        child
      else
        ArcadeEntrance(index: i++, child: child),
  ];
}
