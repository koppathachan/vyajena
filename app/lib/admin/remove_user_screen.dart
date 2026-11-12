// app-ui: Admin screens. Remove a user from the group and the Pi.
import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../app/names.dart';
import '../core/core_service.dart';
import '../members/members_screen.dart';

/// Pick a member to remove.
class RemoveUserScreen extends StatefulWidget {
  const RemoveUserScreen({super.key, required this.core});

  final CoreService core;

  @override
  State<RemoveUserScreen> createState() => _RemoveUserScreenState();
}

class _RemoveUserScreenState extends State<RemoveUserScreen> {
  late final UserNames _names = UserNames(widget.core);
  List<MemberUser>? _users;
  String? _error;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    final me = widget.core.user.value;
    final users = byUser(await widget.core.members())
        .where(
          (m) =>
              me == null ||
              m.user.username != me.username ||
              m.user.serverId != me.serverId,
        )
        .toList();
    await _names.load(users.map((m) => m.user));
    if (mounted) setState(() => _users = users);
  }

  Future<void> _remove(MemberUser member) async {
    final name = _names.of(member.user);
    final sure = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('Remove $name?'),
        content: const Text(
          'Their devices leave the group and can no longer connect. You can '
          'invite them again later.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Remove'),
          ),
        ],
      ),
    );
    if (sure != true) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await widget.core.removeUser(member.user);
      await _load();
    } catch (error) {
      if (mounted) setState(() => _error = errorText(error));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final users = _users;
    return Scaffold(
      appBar: AppBar(title: const Text('Remove a user')),
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
            child: switch (users) {
              null => const SizedBox.shrink(),
              [] => const Center(child: Text('Nobody else is in the group')),
              _ => ListView(
                children: [
                  for (final m in users)
                    ListTile(
                      leading: const Icon(Icons.person_outline),
                      title: Text(_names.of(m.user)),
                      subtitle: Text(devicesText(m.devices.length)),
                      trailing: TextButton(
                        onPressed: _busy ? null : () => _remove(m),
                        child: const Text('Remove'),
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
