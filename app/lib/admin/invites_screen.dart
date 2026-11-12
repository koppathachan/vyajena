// app-ui: Admin screens. The unused invites, each with Revoke.
import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../app/shell_app_bar.dart';
import '../chat/time_text.dart';
import '../core/core_service.dart';

/// The unused invites.
class InvitesScreen extends StatefulWidget {
  const InvitesScreen({super.key, required this.core});

  final CoreService core;

  @override
  State<InvitesScreen> createState() => _InvitesScreenState();
}

class _InvitesScreenState extends State<InvitesScreen> {
  List<InviteInfo>? _invites;
  String? _error;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final invites = await widget.core.listInvites();
      if (mounted) {
        setState(() {
          _invites = invites;
          _error = null;
        });
      }
    } catch (error) {
      if (mounted) setState(() => _error = errorText(error));
    }
  }

  Future<void> _revoke(InviteInfo invite) async {
    final sure = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Revoke this invite?'),
        content: Text('${userText(invite.user)} can no longer join with it.'),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Revoke'),
          ),
        ],
      ),
    );
    if (sure != true) return;
    try {
      await widget.core.revokeInvite(invite.code);
    } catch (error) {
      if (mounted) setState(() => _error = errorText(error));
      return;
    }
    await _load();
  }

  @override
  Widget build(BuildContext context) {
    final invites = _invites;
    return Scaffold(
      appBar: AppBar(title: const Text('Invites')),
      body: Column(
        children: [
          if (_error case final error?)
            Padding(
              padding: const EdgeInsets.all(16),
              child: Text(
                error,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ),
          Expanded(
            child: switch (invites) {
              null => const SizedBox.shrink(),
              [] => const Center(child: Text('No unused invites')),
              _ => ListView(
                children: [
                  for (final invite in invites)
                    ListTile(
                      leading: const Icon(Icons.mail_outline),
                      title: Text(userText(invite.user)),
                      subtitle: Text(
                        'Expires ${timeText(context, invite.expiresAtMs)}',
                      ),
                      trailing: TextButton(
                        onPressed: () => _revoke(invite),
                        child: const Text('Revoke'),
                      ),
                    ),
                ],
              ),
            },
          ),
        ],
      ),
    );
  }
}
