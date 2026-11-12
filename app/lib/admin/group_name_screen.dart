// app-ui: Admin screens. Change the group's name (it travels encrypted to
// every member, message-content: Group settings travel encrypted).
import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../core/core_service.dart';

/// Set the group's name.
class GroupNameScreen extends StatefulWidget {
  const GroupNameScreen({super.key, required this.core});

  final CoreService core;

  @override
  State<GroupNameScreen> createState() => _GroupNameScreenState();
}

class _GroupNameScreenState extends State<GroupNameScreen> {
  final _name = TextEditingController();
  bool _loaded = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    widget.core.groupName().then((name) {
      if (!mounted) return;
      setState(() {
        _name.text = name ?? '';
        _loaded = true;
      });
    });
  }

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final name = _name.text.trim();
    try {
      await widget.core.setGroupName(name.isEmpty ? null : name);
      if (mounted) Navigator.of(context).pop();
    } catch (error) {
      if (mounted) setState(() => _error = errorText(error));
    }
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: const Text('Group name')),
    body: ListView(
      padding: const EdgeInsets.all(16),
      children: [
        TextField(
          key: const Key('groupName'),
          controller: _name,
          enabled: _loaded,
          decoration: const InputDecoration(
            labelText: 'Group name',
            border: OutlineInputBorder(),
          ),
        ),
        const SizedBox(height: 16),
        Align(
          alignment: Alignment.centerRight,
          child: FilledButton(
            onPressed: _loaded ? _save : null,
            child: const Text('Save'),
          ),
        ),
        if (_error case final error?)
          Text(
            error,
            style: TextStyle(color: Theme.of(context).colorScheme.error),
          ),
      ],
    ),
  );
}
