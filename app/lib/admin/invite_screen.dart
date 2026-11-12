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
    });
    try {
      final invite = await widget.core.createInvite(username);
      if (mounted) setState(() => _invite = invite);
    } catch (error) {
      if (mounted) setState(() => _error = errorText(error));
    } finally {
      if (mounted) setState(() => _busy = false);
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
