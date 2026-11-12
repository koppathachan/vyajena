// app-ui: Admin screens. On the admin's first start: create the group,
// with this device as its only member.
import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../app/shell_app_bar.dart';
import '../core/core_service.dart';

/// Create the group.
class CreateGroupScreen extends StatefulWidget {
  const CreateGroupScreen({
    super.key,
    required this.core,
    required this.onCreated,
  });

  final CoreService core;

  /// Called once the group exists.
  final VoidCallback onCreated;

  @override
  State<CreateGroupScreen> createState() => _CreateGroupScreenState();
}

class _CreateGroupScreenState extends State<CreateGroupScreen> {
  bool _creating = false;
  String? _error;

  Future<void> _create() async {
    setState(() {
      _creating = true;
      _error = null;
    });
    try {
      await widget.core.createGroup();
      widget.onCreated();
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
    appBar: shellAppBar(widget.core),
    body: Center(
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              'You are the admin. Create the group, then invite people.',
              textAlign: TextAlign.center,
              style: Theme.of(context).textTheme.titleMedium,
            ),
            const SizedBox(height: 16),
            FilledButton(
              onPressed: _creating ? null : _create,
              child: Text(_creating ? 'Creating…' : 'Create the group'),
            ),
            if (_error case final error?) ...[
              const SizedBox(height: 12),
              Text(
                error,
                textAlign: TextAlign.center,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ],
          ],
        ),
      ),
    ),
  );
}
