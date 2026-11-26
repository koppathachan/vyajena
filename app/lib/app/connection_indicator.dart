// app-ui: Foreground and background: a small connected / connecting /
// offline indicator for the app bar. Connecting pulses in an active colour
// so it never reads as the muted, static offline state (connect-feedback
// design D1); tapping it while connecting or offline retries at once
// (connect-feedback design D2). Once this device was removed from the group
// it says so instead (app-ui: Notices in the chat, "Bob is removed").
import 'dart:async';

import 'package:flutter/material.dart';

import 'arcade/steps.dart';

import '../core/core_service.dart';

/// Shows whether the core is connected, and retries on tap.
class ConnectionIndicator extends StatefulWidget {
  const ConnectionIndicator({super.key, required this.core});

  final CoreService core;

  /// The label for [status], also the indicator's tooltip.
  static String label(ConnectionStatus status) => switch (status) {
    ConnectionStatus.connected => 'Connected',
    ConnectionStatus.connecting => 'Connecting…',
    ConnectionStatus.disconnected => 'Offline',
  };

  /// The label once this device was removed from the group.
  static const removedLabel = 'Removed from the group';

  /// The icon once this device was removed from the group.
  static const removedIcon = Icons.person_off_outlined;

  /// The icon for [status].
  static IconData icon(ConnectionStatus status) => switch (status) {
    ConnectionStatus.connected => Icons.cloud_done_outlined,
    ConnectionStatus.connecting => Icons.cloud_sync_outlined,
    ConnectionStatus.disconnected => Icons.cloud_off_outlined,
  };

  /// The colour for [status] (or the removed state): the active `primary`
  /// while connected or connecting, the muted `onSurfaceVariant` while
  /// offline, the `error` colour once removed (connect-feedback design D1).
  static Color color(
    ColorScheme scheme,
    ConnectionStatus status, {
    required bool removed,
  }) {
    if (removed) return scheme.error;
    return switch (status) {
      ConnectionStatus.connected ||
      ConnectionStatus.connecting => scheme.primary,
      ConnectionStatus.disconnected => scheme.onSurfaceVariant,
    };
  }

  @override
  State<ConnectionIndicator> createState() => _ConnectionIndicatorState();
}

class _ConnectionIndicatorState extends State<ConnectionIndicator>
    with SingleTickerProviderStateMixin {
  late final AnimationController _pulse = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 900),
  );
  late final Animation<double> _opacity = Tween<double>(
    begin: 0.35,
    end: 1,
  ).animate(CurvedAnimation(parent: _pulse, curve: const Steps(4)));

  late final Listenable _listenable = Listenable.merge([
    widget.core.connection,
    widget.core.removed,
  ]);

  @override
  void initState() {
    super.initState();
    _listenable.addListener(_onChange);
    _syncAnimation();
  }

  void _onChange() {
    _syncAnimation();
    if (mounted) setState(() {});
  }

  /// Pulses only while connecting; stops and resets to fully opaque
  /// otherwise, so no ticker runs in the other states.
  void _syncAnimation() {
    final connecting =
        !widget.core.removed.value &&
        widget.core.connection.value == ConnectionStatus.connecting;
    // No pulse under the system's reduce-motion setting (Arcade motion).
    final reduce = WidgetsBinding
        .instance
        .platformDispatcher
        .accessibilityFeatures
        .disableAnimations;
    if (connecting && !reduce) {
      if (!_pulse.isAnimating) _pulse.repeat(reverse: true);
    } else if (_pulse.isAnimating || _pulse.value != 1) {
      _pulse.stop();
      _pulse.value = 1;
    }
  }

  @override
  void dispose() {
    _listenable.removeListener(_onChange);
    _pulse.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final status = widget.core.connection.value;
    final removed = widget.core.removed.value;
    final text = removed
        ? ConnectionIndicator.removedLabel
        : ConnectionIndicator.label(status);
    // Nothing to retry once connected, and the core will not reconnect once
    // this device was removed (connect-feedback design D2).
    final canRetry = !removed && status != ConnectionStatus.connected;

    return Tooltip(
      message: text,
      child: Semantics(
        label: text,
        button: canRetry,
        child: InkResponse(
          radius: 24,
          onTap: canRetry ? () => unawaited(widget.core.reconnectNow()) : null,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12),
            child: FadeTransition(
              opacity: _opacity,
              child: Icon(
                removed
                    ? ConnectionIndicator.removedIcon
                    : ConnectionIndicator.icon(status),
                size: 20,
                color: ConnectionIndicator.color(
                  scheme,
                  status,
                  removed: removed,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
