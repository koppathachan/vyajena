// app-ui: Admin screens. Invite a user by username; show the invite text
// to share. Once they join, the admin's device adds them on its own
// (group-membership: Automatic add of new users).
import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../app/shell_app_bar.dart';
import '../app/text_box.dart';
import '../core/core_service.dart';

/// How long until an invite expires, for display (app-ui: Invite expiry is
/// surfaced). An invite is valid for `invite_ttl` (7 days by default).
String inviteExpiryText(int expiresAtMs, {DateTime? now}) {
  final current = now ?? DateTime.now();
  final remaining = DateTime.fromMillisecondsSinceEpoch(
    expiresAtMs,
  ).difference(current);
  if (!remaining.isNegative && remaining.inDays >= 1) {
    final days = remaining.inDays;
    return 'Expires in ${days == 1 ? '1 day' : '$days days'}.';
  }
  if (!remaining.isNegative && remaining.inHours >= 1) {
    final hours = remaining.inHours;
    return 'Expires in ${hours == 1 ? '1 hour' : '$hours hours'}.';
  }
  if (!remaining.isNegative) {
    final minutes = remaining.inMinutes < 1 ? 1 : remaining.inMinutes;
    return 'Expires in ${minutes == 1 ? '1 minute' : '$minutes minutes'}.';
  }
  return 'This invite has expired — create a new one.';
}

/// Create an invite.
class InviteScreen extends StatefulWidget {
  const InviteScreen({super.key, required this.core});

  final CoreService core;

  @override
  State<InviteScreen> createState() => _InviteScreenState();
}

class _InviteScreenState extends State<InviteScreen> {
  final _username = TextEditingController();
  InviteInfo? _invite;
  String? _error;
  bool _busy = false;

  /// Whether the invite just shown was an existing unused one for this
  /// username, not a freshly minted one (the server keeps at most one pending
  /// invite per username). Drives the "already existed" note.
  bool _existed = false;

  @override
  void dispose() {
    _username.dispose();
    super.dispose();
  }

  Future<void> _create() async {
    final username = _username.text.trim();
    if (username.isEmpty) return;
    setState(() {
      _busy = true;
      _error = null;
      _invite = null;
      _existed = false;
    });
    try {
      // The server returns the existing unused invite for a username instead of
      // minting a duplicate; check first so the admin is told when that is what
      // they got, rather than it happening silently.
      final existing = await _hasUnusedInvite(username);
      final invite = await widget.core.createInvite(username);
      if (mounted) {
        setState(() {
          _invite = invite;
          _existed = existing;
        });
      }
    } catch (error) {
      if (mounted) {
        setState(() {
          // The server refuses an invite for a username whose user still
          // exists and is not removed. Tell the admin to remove them first —
          // the recovery flow for a wiped device (app-ui: Admin recovers a
          // locked-out user).
          _error = error is CoreError && error.kind == CoreErrorKind.refused
              ? '$username may already be in the group. To re-invite someone '
                    'whose phone was wiped or cannot re-sync, remove them first '
                    '(Remove a user), then invite the same username here.'
              : errorText(error);
        });
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// Whether an unused invite for [username] already exists (best-effort; the
  /// server dedupes regardless).
  Future<bool> _hasUnusedInvite(String username) async {
    try {
      final invites = await widget.core.listInvites();
      return invites.any((i) => i.user.username == username);
    } on Object {
      return false;
    }
  }

  @override
  Widget build(BuildContext context) {
    final invite = _invite;
    final text = invite?.text;
    return Scaffold(
      appBar: AppBar(title: const Text('Invite a user')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Card.outlined(
            key: const Key('recovery-note'),
            child: const Padding(
              padding: EdgeInsets.all(12),
              child: Text(
                'Lost a phone or stuck out of sync? To let someone back in, '
                'first remove them (Remove a user), then invite the same '
                'username here. They lose their old device key and the history '
                'on that device, and any direct messages they had reopen.',
              ),
            ),
          ),
          const SizedBox(height: 12),
          TextField(
            key: const Key('username'),
            controller: _username,
            autocorrect: false,
            decoration: const InputDecoration(
              labelText: 'Username',
              helperText: 'Their id will be username@server.',
              border: OutlineInputBorder(),
            ),
            onSubmitted: (_) => _create(),
          ),
          const SizedBox(height: 12),
          Align(
            alignment: Alignment.centerRight,
            child: FilledButton(
              onPressed: _busy ? null : _create,
              child: const Text('Invite'),
            ),
          ),
          if (_error case final error?)
            Text(
              error,
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
          if (invite != null && text != null) ...[
            const SizedBox(height: 16),
            if (_existed) ...[
              Card.outlined(
                key: const Key('invite-existed'),
                child: ListTile(
                  leading: const Icon(Icons.info_outline),
                  title: Text(
                    '${userText(invite.user)} already had an unused invite — '
                    'this is that same one. Revoke it in Invites first if you '
                    'want a fresh one.',
                  ),
                ),
              ),
              const SizedBox(height: 12),
            ],
            Text(
              'Send this invite to ${userText(invite.user)}. It works once. '
              'When they join, this phone adds them to the group.',
            ),
            const SizedBox(height: 8),
            ShareableText(text: text),
            const SizedBox(height: 8),
            Text(
              inviteExpiryText(invite.expiresAtMs),
              key: const Key('invite-expiry'),
              style: Theme.of(context).textTheme.bodySmall,
            ),
          ],
        ],
      ),
    );
  }
}
