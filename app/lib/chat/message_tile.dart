// app-ui: Chat screen. One message: its sender as "Ali (alice)", its text
// (or a placeholder for a body this version does not know), and for own
// messages the send status: ⏳ sending, ✓ accepted, or failed with the
// reason.
import 'package:flutter/material.dart';

import '../core/core_service.dart';

/// Placeholder for a message body this version does not know.
const unsupportedText =
    'This message needs a newer version of the app to be shown.';

/// The status mark of an own message.
String statusText(SendState status) => switch (status) {
  SendState_Sending() => '⏳',
  SendState_Accepted() => '✓',
  SendState_Failed(:final reason) => 'Failed: $reason',
};

/// One message row.
class MessageTile extends StatelessWidget {
  const MessageTile({super.key, required this.message, required this.sender});

  final ChatMessage message;

  /// How to show the sender, "Ali (alice)".
  final String sender;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final text = Theme.of(context).textTheme;
    final own = message.outgoing;
    final status = message.status;
    final body = switch (message.body) {
      ChatBody_Text(:final text) => Text(text),
      ChatBody_Unsupported() => Text(
        unsupportedText,
        style: TextStyle(
          fontStyle: FontStyle.italic,
          color: scheme.onSurfaceVariant,
        ),
      ),
    };
    return Align(
      alignment: own ? Alignment.centerRight : Alignment.centerLeft,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 480),
        child: Card(
          color: own ? scheme.secondaryContainer : scheme.surfaceContainerHigh,
          elevation: 0,
          margin: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  sender,
                  style: text.labelMedium?.copyWith(color: scheme.primary),
                ),
                const SizedBox(height: 2),
                body,
                if (own && status != null) ...[
                  const SizedBox(height: 2),
                  Align(
                    alignment: Alignment.centerRight,
                    child: Text(
                      statusText(status),
                      style: text.labelSmall?.copyWith(
                        color: status is SendState_Failed
                            ? scheme.error
                            : scheme.onSurfaceVariant,
                      ),
                    ),
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}
