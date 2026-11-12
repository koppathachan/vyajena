// app-ui: Notices in the chat: gap, removed for inactivity (with the time
// messages stop), rejoined, a changed identity key naming the user (with
// Accept on admin devices), and messages that could not be read.
import 'package:flutter/material.dart';

import 'chat_model.dart';
import 'time_text.dart';

/// The text of [notice]; [name] shows a user.
String noticeText(
  BuildContext context,
  Notice notice,
  String Function(Object user) name,
) => switch (notice) {
  GapNotice(:final newestExpiredAtMs) =>
    'Messages up to ${timeText(context, newestExpiredAtMs)} expired before '
        'this phone fetched them.',
  RemovedNotice(:final missedSinceMs) =>
    'This phone was away too long and was removed from the group. Messages '
        'since ${timeText(context, missedSinceMs)} were not kept.',
  RejoinedNotice() => 'This phone is back in the group.',
  KeyChangedNotice(:final user, :final accepted) =>
    accepted
        ? "${name(user)}'s new key was accepted."
        : "${name(user)}'s key changed. Make sure it is really them.",
  UnreadableNotice(:final sender) =>
    sender == null
        ? 'A message could not be read.'
        : 'A message from ${name(sender)} could not be read.',
  CommitRefusedNotice(:final reason) =>
    'This phone stopped following the group until it is added again: '
        '$reason',
  ServerIdentityNotice() =>
    "The server's identity does not match. Not connecting: someone may be "
        'pretending to be the server.',
};

/// One notice row.
class NoticeTile extends StatelessWidget {
  const NoticeTile({
    super.key,
    required this.notice,
    required this.text,
    this.onAccept,
  });

  final Notice notice;
  final String text;

  /// Shown as an Accept button (admin devices, a changed key).
  final VoidCallback? onAccept;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final warning =
        notice is KeyChangedNotice && !(notice as KeyChangedNotice).accepted ||
        notice is ServerIdentityNotice ||
        notice is CommitRefusedNotice;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 6),
      child: Card.outlined(
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: Column(
            children: [
              Row(
                children: [
                  Icon(
                    warning ? Icons.warning_amber_outlined : Icons.info_outline,
                    size: 18,
                    color: warning ? scheme.error : scheme.onSurfaceVariant,
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      text,
                      style: Theme.of(context).textTheme.bodySmall,
                    ),
                  ),
                ],
              ),
              if (onAccept case final accept?)
                Align(
                  alignment: Alignment.centerRight,
                  child: TextButton(
                    onPressed: accept,
                    child: const Text('Accept new key'),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}
