// app-ui: Foreground and background: a small connected / connecting /
// offline indicator for the app bar; once this device was removed from the
// group it says so instead (app-ui: Notices in the chat, "Bob is removed").
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

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return ListenableBuilder(
      listenable: Listenable.merge([core.connection, core.removed]),
      builder: (context, _) {
        final status = core.connection.value;
        final removed = core.removed.value;
        final text = removed ? removedLabel : label(status);
        return Tooltip(
          message: text,
          child: Semantics(
            label: text,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12),
              child: Icon(
                removed ? removedIcon : icon(status),
                size: 20,
                color: removed
                    ? scheme.error
                    : status == ConnectionStatus.connected
                    ? scheme.primary
                    : scheme.onSurfaceVariant,
              ),
            ),
          ),
        );
      },
    );
  }
}
