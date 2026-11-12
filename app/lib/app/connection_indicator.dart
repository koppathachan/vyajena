// app-ui: Foreground and background: a small connected / connecting /
// offline indicator for the app bar.
import 'package:flutter/material.dart';

import '../core/core_service.dart';

/// Shows whether the core is connected.
class ConnectionIndicator extends StatelessWidget {
  const ConnectionIndicator({super.key, required this.core});

  final CoreService core;

  /// The label for [status], also the indicator's tooltip.
  static String label(ConnectionStatus status) => switch (status) {
    ConnectionStatus.connected => 'Connected',
    ConnectionStatus.connecting => 'Connecting…',
    ConnectionStatus.disconnected => 'Offline',
  };

  /// The icon for [status].
  static IconData icon(ConnectionStatus status) => switch (status) {
    ConnectionStatus.connected => Icons.cloud_done_outlined,
    ConnectionStatus.connecting => Icons.cloud_sync_outlined,
    ConnectionStatus.disconnected => Icons.cloud_off_outlined,
  };

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return ValueListenableBuilder<ConnectionStatus>(
      valueListenable: core.connection,
      builder: (context, status, _) => Tooltip(
        message: label(status),
        child: Semantics(
          label: label(status),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12),
            child: Icon(
              icon(status),
              size: 20,
              color: status == ConnectionStatus.connected
                  ? scheme.primary
                  : scheme.onSurfaceVariant,
            ),
          ),
        ),
      ),
    );
  }
}
