// app-ui: Admin screens. Invite a user by username; show the invite text
// to share. Once they join, the admin's device adds them on its own
// (group-membership: Automatic add of new users).
import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../app/shell_app_bar.dart';
import '../app/text_box.dart';
import '../core/core_service.dart';

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
      if (mounted) setState(() => _error = errorText(error));
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
          ],
        ],
      ),
    );
  }
}
