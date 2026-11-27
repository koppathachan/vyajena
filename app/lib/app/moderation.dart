// app-ui: Safety and moderation (change `report-and-block`). The shared report
// and block affordances — the reason prompt, the pre-block confirm — and the
// published moderation policy text, so the message menu, the members list and
// the Profile tab's "Safety & moderation" entry all show the same thing.
import 'package:flutter/material.dart';

import '../chat/message_tile.dart' show reportLabel, blockLabel;

/// The safety contact shown in the policy (moderation: Published moderation
/// policy). This is the ONE place to change it: the owner supplies the real,
/// monitored safety address and replaces this token with it (one line, nothing
/// else — the policy text and the tests read this constant) before the public
/// release. Do not invent an address.
const safetyContactPlaceholder = 'support@purplechaamba.com';

/// The published moderation policy, shown in the Profile tab's "Safety &
/// moderation" entry and copied to both store listings (moderation: Published
/// moderation policy; design.md). The contact line carries the
/// [safetyContactPlaceholder] token until the owner supplies the real address.
const moderationPolicy =
    'vyajena2 is an invite-only, end-to-end encrypted chat. Messages are '
    'encrypted on your device; we (the operators) cannot read them.\n\n'
    'Block. You can block any user from a message or from their profile. '
    'Blocking hides that user’s messages and direct messages in the app on '
    'your device. It does not yet stop notifications: a push from a blocked '
    'user can still show a generic “New message”. You can unblock them at any '
    'time.\n\n'
    'Report. You can report a message or a user to the group’s '
    'administrator from the message menu or the user’s profile. Your '
    'report is sent privately to the administrator; for a direct message it '
    'includes a short quote of the reported content so the administrator can '
    'review it.\n\n'
    'We act on reports. The administrator reviews every report and, within 24 '
    'hours, removes content that violates these rules and ejects the user who '
    'posted it. Removal uses the administrator’s existing ability to '
    'remove any member.\n\n'
    'No tolerance for harassment, threats, sexual content involving minors, or '
    'other illegal or abusive content.\n\n'
    'Contact. Report safety concerns to $safetyContactPlaceholder.';

/// Asks for a short report reason (moderation: Report a message or a user);
/// `null` if cancelled. The reason is never empty (a blank one becomes a single
/// space) so a report always carries this field.
Future<String?> askReportReason(BuildContext context) {
  final controller = TextEditingController();
  return showDialog<String>(
    context: context,
    builder: (dialog) => AlertDialog(
      scrollable: true,
      key: const Key('report-reason'),
      title: const Text('Report to the admin'),
      content: TextField(
        key: const Key('report-reason-field'),
        controller: controller,
        autofocus: true,
        decoration: const InputDecoration(
          hintText: 'Why are you reporting this?',
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(dialog).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const Key('report-reason-send'),
          onPressed: () => Navigator.of(dialog).pop(
            controller.text.trim().isEmpty ? ' ' : controller.text.trim(),
          ),
          child: const Text(reportLabel),
        ),
      ],
    ),
  );
}

/// Confirms blocking [name], warning it hides them on this device only
/// (moderation: Block a user; app-ui: Block and unblock a user). `true` to
/// proceed.
Future<bool> confirmBlock(BuildContext context, String name) async {
  final ok = await showDialog<bool>(
    context: context,
    builder: (dialog) => AlertDialog(
      scrollable: true,
      key: const Key('block-confirm'),
      title: Text('Block $name?'),
      content: const Text(
        'Blocking hides this user’s messages and DMs on this device only. '
        'You can unblock them at any time.',
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(dialog).pop(false),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const Key('block-confirm-ok'),
          onPressed: () => Navigator.of(dialog).pop(true),
          child: const Text(blockLabel),
        ),
      ],
    ),
  );
  return ok ?? false;
}
