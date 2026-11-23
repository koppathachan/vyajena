// app-ui: Profile and members. This user's display name and about text;
// the username is shown read-only (the admin chose it).
//
// app-shell: Profile tab. Embedded in the bottom navigation it also shows the
// server info (server id and connection state) and the settings entries, and
// stays open after a save instead of closing.
import 'package:flutter/material.dart';

import '../app/connection_indicator.dart';
import '../app/errors.dart';
import '../app/routes.dart';
import '../app/shell_app_bar.dart';
import '../core/core_service.dart';
import '../shell/avatars.dart';

/// Edit this user's profile.
class ProfileScreen extends StatefulWidget {
  const ProfileScreen({
    super.key,
    required this.core,
    this.embedded = false,
    this.admin = false,
  });

  final CoreService core;

  /// Whether this is the Profile tab (app-shell) rather than the pushed
  /// screen.
  final bool embedded;

  /// Whether this device's user is the admin.
  final bool admin;

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
      if (!mounted) return;
      if (widget.embedded) {
        setState(() => _saving = false);
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(const SnackBar(content: Text('Profile saved')));
      } else {
        Navigator.of(context).pop();
      }
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
      appBar: widget.embedded
          ? shellAppBar(widget.core, title: 'Profile')
          : AppBar(title: const Text('Profile')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          if (widget.embedded && user != null) ...[
            Center(
              child: NameAvatar(
                key: const Key('profile-header-avatar'),
                name: _name.text.trim().isEmpty ? user.username : _name.text,
                seed: user.username,
                radius: 44,
              ),
            ),
            const SizedBox(height: 16),
          ],
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
          if (widget.embedded) ..._serverAndSettings(context, user),
        ],
      ),
    );
  }

  /// The server info and the settings entries (app-shell: Profile tab).
  List<Widget> _serverAndSettings(BuildContext context, UserRef? user) => [
    const SizedBox(height: 24),
    Text('Server', style: Theme.of(context).textTheme.titleSmall),
    ListTile(
      key: const Key('server-id'),
      contentPadding: EdgeInsets.zero,
      leading: const Icon(Icons.dns_outlined),
      title: const Text('Server id'),
      subtitle: Text(user?.serverId ?? ''),
    ),
    ValueListenableBuilder<ConnectionStatus>(
      valueListenable: widget.core.connection,
      builder: (context, status, _) => ListTile(
        key: const Key('server-connection'),
        contentPadding: EdgeInsets.zero,
        leading: Icon(ConnectionIndicator.icon(status)),
        title: const Text('Connection'),
        subtitle: Text(ConnectionIndicator.label(status)),
      ),
    ),
    const SizedBox(height: 8),
    Text('Settings', style: Theme.of(context).textTheme.titleSmall),
    ListTile(
      key: const Key('settings-link-device'),
      contentPadding: EdgeInsets.zero,
      leading: const Icon(Icons.devices_outlined),
      title: const Text('Link a new device'),
      onTap: () => Navigator.of(context).pushNamed(Routes.approveLink),
    ),
    ListTile(
      key: const Key('settings-members'),
      contentPadding: EdgeInsets.zero,
      leading: const Icon(Icons.people_outline),
      title: const Text('Members and devices'),
      onTap: () => Navigator.of(context).pushNamed(Routes.members),
    ),
  ];
}
