// app-ui: Profile and members. This user's display name and about text;
// the username is shown read-only (the admin chose it).
import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../app/shell_app_bar.dart';
import '../core/core_service.dart';

/// Edit this user's profile.
class ProfileScreen extends StatefulWidget {
  const ProfileScreen({super.key, required this.core});

  final CoreService core;

  @override
  State<ProfileScreen> createState() => _ProfileScreenState();
}

class _ProfileScreenState extends State<ProfileScreen> {
  final _name = TextEditingController();
  final _about = TextEditingController();
  bool _loaded = false;
  bool _saving = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _name.dispose();
    _about.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    final user = widget.core.user.value;
    if (user == null) return;
    final profile = await widget.core.profile(user);
    if (!mounted) return;
    setState(() {
      _name.text = profile.displayName ?? '';
      _about.text = profile.about ?? '';
      _loaded = true;
    });
  }

  static String? _orNull(String text) =>
      text.trim().isEmpty ? null : text.trim();

  Future<void> _save() async {
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await widget.core.setProfile(
        displayName: _orNull(_name.text),
        about: _orNull(_about.text),
      );
      if (mounted) Navigator.of(context).pop();
    } catch (error) {
      if (mounted) {
        setState(() {
          _saving = false;
          _error = errorText(error);
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final user = widget.core.user.value;
    return Scaffold(
      appBar: AppBar(title: const Text('Profile')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          TextFormField(
            key: const Key('username'),
            initialValue: user == null ? '' : userText(user),
            readOnly: true,
            enabled: false,
            decoration: const InputDecoration(
              labelText: 'Username',
              helperText: 'Chosen by the admin; it cannot be changed.',
            ),
          ),
          const SizedBox(height: 16),
          TextField(
            key: const Key('displayName'),
            controller: _name,
            enabled: _loaded,
            decoration: const InputDecoration(
              labelText: 'Display name',
              border: OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 16),
          TextField(
            key: const Key('about'),
            controller: _about,
            enabled: _loaded,
            minLines: 2,
            maxLines: 4,
            decoration: const InputDecoration(
              labelText: 'About',
              border: OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 16),
          Align(
            alignment: Alignment.centerRight,
            child: FilledButton(
              onPressed: _loaded && !_saving ? _save : null,
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
}
