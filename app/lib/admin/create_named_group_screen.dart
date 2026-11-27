// app-ui: Admin screens (change `lobby-multi-group`). "Create group": the
// admin names the group (optional) and picks its members from the server's
// members, then the core creates a named group and adds them.
import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../app/names.dart';
import '../core/core_service.dart';
import '../members/member.dart';

/// Create a named group by picking its members; pops with the new chat id.
class CreateNamedGroupScreen extends StatefulWidget {
  const CreateNamedGroupScreen({super.key, required this.core});

  final CoreService core;

  @override
  State<CreateNamedGroupScreen> createState() => _CreateNamedGroupScreenState();
}

class _CreateNamedGroupScreenState extends State<CreateNamedGroupScreen> {
  late final UserNames _names = UserNames(widget.core);
  final _name = TextEditingController();
  List<UserRef> _candidates = const [];
  final Set<String> _picked = {};
  bool _creating = false;
  String? _error;

  static String _key(UserRef user) => '${user.username}@${user.serverId}';

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    try {
      final me = widget.core.user.value;
      final candidates = [
        for (final m in byUser(await widget.core.members()))
          if (me == null || _key(m.user) != _key(me)) m.user,
      ];
      await _names.load(candidates);
      if (mounted) setState(() => _candidates = candidates);
    } catch (error) {
      if (mounted) setState(() => _error = errorText(error));
    }
  }

  Future<void> _create() async {
    setState(() {
      _creating = true;
      _error = null;
    });
    try {
      final name = _name.text.trim();
      final chatId = await widget.core.createNamedGroup(
        name.isEmpty ? null : name,
        [
          for (final user in _candidates)
            if (_picked.contains(_key(user))) user,
        ],
      );
      if (mounted) Navigator.of(context).pop(chatId);
    } catch (error) {
      if (mounted) {
        setState(() {
          _creating = false;
          _error = errorText(error);
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: const Text('Create group')),
    body: ListView(
      padding: const EdgeInsets.all(16),
      children: [
        TextField(
          key: const Key('new-group-name'),
          controller: _name,
          decoration: const InputDecoration(labelText: 'Group name'),
        ),
        const SizedBox(height: 16),
        Text('Members', style: Theme.of(context).textTheme.titleSmall),
        for (final user in _candidates)
          CheckboxListTile(
            key: ValueKey('member-${user.username}'),
            value: _picked.contains(_key(user)),
            title: Text(_names.of(user)),
            onChanged: (on) => setState(() {
              if (on ?? false) {
                _picked.add(_key(user));
              } else {
                _picked.remove(_key(user));
              }
            }),
          ),
        if (_error case final error?)
          Padding(
            padding: const EdgeInsets.only(top: 12),
            child: Text(
              error,
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
          ),
        const SizedBox(height: 16),
        FilledButton(
          key: const Key('create-group-confirm'),
          onPressed: _creating ? null : _create,
          child: Text(_creating ? 'Creating…' : 'Create group'),
        ),
      ],
    ),
  );
}
