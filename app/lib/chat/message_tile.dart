// app-ui: Chat screen. One message. Another user's message shows its sender
// as "Ali (alice)" and sits on the left. A message from this user (any of
// their devices, `ownUser`) sits on the right, sized to its text, with no
// sender label; one this device sent (`outgoing`) also shows its send
// status inline at the end of the bubble: ⏳ sending, ✓ accepted, or failed
// with the reason (qa-fixes design D1). A body this version does not know
// shows a placeholder.
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
    final own = message.ownUser;
    final status = message.outgoing ? message.status : null;
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
    // Everything here sizes to its content: a widget that fills the width
    // it is given (an Align, say) would stretch the bubble across the
    // screen (design D1).
    final content = own
        ? Wrap(
            alignment: WrapAlignment.end,
            crossAxisAlignment: WrapCrossAlignment.end,
            spacing: 8,
            children: [
              body,
              if (status != null)
                Text(
                  statusText(status),
                  style: text.labelSmall?.copyWith(
                    color: status is SendState_Failed
                        ? scheme.error
                        : scheme.onSurfaceVariant,
                  ),
                ),
            ],
          )
        : Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                sender,
                style: text.labelMedium?.copyWith(color: scheme.primary),
              ),
              const SizedBox(height: 2),
              body,
            ],
          );
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
            child: content,
          ),
        ),
      ),
    );
  }
}
