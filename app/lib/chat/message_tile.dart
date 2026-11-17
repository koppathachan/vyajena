// app-ui: Chat screen. One message. Another user's message shows its sender
// as "Ali (alice)" and sits on the left. A message from this user (any of
// their devices, `ownUser`) sits on the right, sized to its text, with no
// sender label; one this device sent (`outgoing`) also shows its send
// status inline at the end of the bubble: ⏳ sending, ✓ accepted, or failed
// with the reason (qa-fixes design D1). A body this version does not know
// shows a placeholder.
//
// app-ui: Mentions of me; Reply to a message (change `mentions`). Mentions
// are drawn with the reader's name for their target, mentions of this user
// highlighted. A reply shows who and what it answers above its text; the
// quote is tappable. Swiping right or long-pressing offers Reply.
import 'package:flutter/material.dart';

import '../core/core_service.dart';
import 'mentions.dart';

/// Placeholder for a message body this version does not know.
const unsupportedText =
    'This message needs a newer version of the app to be shown.';

/// The status mark of an own message.
String statusText(SendState status) => switch (status) {
  SendState_Sending() => '⏳',
  SendState_Accepted() => '✓',
  SendState_Failed(:final reason) => 'Failed: $reason',
};

/// What the long-press sheet's reply entry says.
const replyLabel = 'Reply';

/// One message row.
class MessageTile extends StatelessWidget {
  const MessageTile({
    super.key,
    required this.message,
    required this.sender,
    this.mentionLabel,
    this.me,
    this.replyTo,
    this.onReply,
    this.onQuoteTap,
  });

  final ChatMessage message;

  /// How to show the sender, "Ali (alice)".
  final String sender;

  /// The name drawn after a mention's "@"; the username when unset.
  final String Function(MentionTarget target)? mentionLabel;

  /// This device's user, whose mentions are highlighted.
  final UserRef? me;

  /// How to show the user a reply answers.
  final String? replyTo;

  /// Starts a reply to this message (swipe right, or long-press → Reply).
  final VoidCallback? onReply;

  /// The reply's quote was tapped.
  final VoidCallback? onQuoteTap;

  String _label(MentionTarget target) =>
      mentionLabel?.call(target) ??
      switch (target) {
        MentionTarget_User(:final user) => user.username,
        MentionTarget_Everyone() => everyoneLabel,
      };

  Future<void> _sheet(BuildContext context) async {
    final reply = await showModalBottomSheet<bool>(
      context: context,
      builder: (context) => SafeArea(
        child: ListTile(
          leading: const Icon(Icons.reply),
          title: const Text(replyLabel),
          onTap: () => Navigator.of(context).pop(true),
        ),
      ),
    );
    if (reply ?? false) onReply?.call();
  }

  Widget _quote(BuildContext context, ReplyQuote reply) {
    final scheme = Theme.of(context).colorScheme;
    final text = Theme.of(context).textTheme;
    return InkWell(
      key: ValueKey('quote-${message.id}'),
      onTap: onQuoteTap,
      child: Container(
        margin: const EdgeInsets.only(bottom: 4),
        padding: const EdgeInsets.fromLTRB(8, 4, 8, 4),
        decoration: BoxDecoration(
          border: Border(left: BorderSide(color: scheme.primary, width: 3)),
          color: scheme.surfaceContainerHighest,
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              replyTo ?? reply.to.username,
              style: text.labelSmall?.copyWith(color: scheme.primary),
            ),
            Text(
              reply.quote.isEmpty ? unsupportedText : reply.quote,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: text.bodySmall?.copyWith(color: scheme.onSurfaceVariant),
            ),
          ],
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final text = Theme.of(context).textTheme;
    final own = message.ownUser;
    final status = message.outgoing ? message.status : null;
    final textBody = switch (message.body) {
      ChatBody_Text(:final text) =>
        message.mentions.isEmpty
            ? Text(text)
            : Text.rich(
                mentionSpans(
                  context,
                  text,
                  message.mentions,
                  label: _label,
                  me: me,
                  own: message.ownUser,
                ),
              ),
      ChatBody_Unsupported() => Text(
        unsupportedText,
        style: TextStyle(
          fontStyle: FontStyle.italic,
          color: scheme.onSurfaceVariant,
        ),
      ),
    };
    final reply = message.reply;
    final body = reply == null
        ? textBody
        : Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [_quote(context, reply), textBody],
          );
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
    final bubble = Align(
      alignment: own ? Alignment.centerRight : Alignment.centerLeft,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 480),
        child: Card(
          color: own ? scheme.secondaryContainer : scheme.surfaceContainerHigh,
          elevation: 0,
          shape: message.mentionsMe
              ? RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(12),
                  side: BorderSide(color: scheme.tertiary, width: 1.5),
                )
              : null,
          margin: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
            child: content,
          ),
        ),
      ),
    );
    final startReply = onReply;
    if (startReply == null) return bubble;
    // Swipe right to reply: the row slides, then springs back (it is never
    // dismissed).
    return Dismissible(
      key: ValueKey('swipe-${message.id}'),
      direction: DismissDirection.startToEnd,
      dismissThresholds: const {DismissDirection.startToEnd: 0.25},
      confirmDismiss: (_) async {
        startReply();
        return false;
      },
      background: Align(
        alignment: Alignment.centerLeft,
        child: Padding(
          padding: const EdgeInsets.only(left: 16),
          child: Icon(Icons.reply, color: scheme.primary),
        ),
      ),
      child: GestureDetector(onLongPress: () => _sheet(context), child: bubble),
    );
  }
}
