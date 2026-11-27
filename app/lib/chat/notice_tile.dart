// app-ui: Notices in the chat: gap, removed for inactivity (with the time
// messages stop), rejoined, a changed safety number naming the user (with a
// Re-trust action every member can take, admin-recovery), messages that could
// not be read, this device removed, a group rename, and membership changes
// (qa-fixes design D5).
import 'package:flutter/material.dart';

import 'chat_model.dart';
import 'time_text.dart';

/// What the chat says once this device was removed from the group.
const deviceRemovedText = 'This device was removed from the group.';

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
  KeyChangedNotice(:final user, :final acknowledged) =>
    acknowledged
        ? "You re-trusted ${name(user)}'s new safety number."
        : "⚠️ ${name(user)}'s safety number changed — re-trust?",
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
  DeviceRemovedNotice() => deviceRemovedText,
  MemberNotice(:final change, :final user, :final by) => switch (change) {
    MemberChange.joined => '${name(user)} joined',
    MemberChange.addedDevice => '${name(user)} added a device',
    MemberChange.removedDevice => '${name(user)} removed a device',
    MemberChange.removedDeviceOf =>
      '${name(by)} removed a device of ${name(user)}',
    MemberChange.removedUser => '${name(by)} removed ${name(user)}',
  },
  InactivityNotice(:final user, :final returned) =>
    returned
        ? '${name(user)} came back and was re-added to the group.'
        // The client cannot tell an inactivity removal from a deleted account
        // (both are the server's external Remove), so the line names both.
        : '${name(user)} was removed from the group automatically '
              '(inactive or account deleted).',
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
        notice is KeyChangedNotice &&
            !(notice as KeyChangedNotice).acknowledged ||
        notice is ServerIdentityNotice ||
        notice is DeviceRemovedNotice ||
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
              if (onAccept case final acknowledge?)
                Align(
                  alignment: Alignment.centerRight,
                  child: TextButton(
                    onPressed: acknowledge,
                    child: const Text('Re-trust'),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}
