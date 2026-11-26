// app-ui: Profile and members. This user's display name and about text;
// the username is shown read-only (the admin chose it).
//
// app-shell: Profile tab. Embedded in the bottom navigation it also shows the
// server info (server id and connection state) and the settings entries, and
// stays open after a save instead of closing.
import 'dart:async';

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

  /// Whether whole-app mute is on (notification-prefs: Mute the whole app).
  bool _appMuted = false;

  /// Guards the mute switch while a toggle is in flight.
  bool _muteBusy = false;

  /// Guards the presence switch while a toggle is in flight.
  bool _presenceBusy = false;
  StreamSubscription<CoreEvent>? _subscription;

  @override
  void initState() {
    super.initState();
    // Keep the mute switch in step with changes from this or another device.
    _subscription = widget.core.events.listen((event) {
      if (event is CoreEvent_MutePrefsChanged) _loadMuted();
    });
    _load();
    _loadMuted();
  }

  @override
  void dispose() {
    _subscription?.cancel();
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

  Future<void> _loadMuted() async {
    try {
      final muted = await widget.core.appMuted();
      if (mounted) setState(() => _appMuted = muted);
    } on Object {
      // Leave the last known value; the switch is not critical.
    }
  }

  Future<void> _setAppMuted(bool muted) async {
    setState(() => _muteBusy = true);
    try {
      await widget.core.setAppMuted(muted);
      if (mounted) setState(() => _appMuted = muted);
    } catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text(errorText(error))));
      }
    } finally {
      if (mounted) setState(() => _muteBusy = false);
    }
  }

  /// manual-presence: Toggle your status on the Profile screen.
  Future<void> _setOnline(bool online) async {
    setState(() => _presenceBusy = true);
    try {
      await widget.core.setMyPresence(online);
    } catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text(errorText(error))));
      }
    } finally {
      if (mounted) setState(() => _presenceBusy = false);
    }
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
        // Drop the keyboard: on the Profile tab it covers the shell's bottom
        // navigation, so leaving it up after a save traps the user with no way
        // back to Chats (app-shell: Profile tab).
        FocusScope.of(context).unfocus();
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
      // On the Profile tab the keyboard hides the shell's bottom navigation,
      // so tapping outside a field must dismiss it to get back to Chats
      // (app-shell: Profile tab).
      body: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: () => FocusScope.of(context).unfocus(),
        child: ListView(
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
      ),
    );
  }

  /// The server info and the settings entries (app-shell: Profile tab).
  List<Widget> _serverAndSettings(BuildContext context, UserRef? user) => [
    const SizedBox(height: 24),
    Text('Presence', style: Theme.of(context).textTheme.titleSmall),
    // manual-presence: Toggle your status on the Profile screen. A sliding
    // toggle the user flips on to appear Online (green) and off to appear
    // Offline (red). Manual and durable: it is never derived from the
    // connection, and others see whatever the user chose.
    ValueListenableBuilder<PresenceStatus>(
      valueListenable: widget.core.myPresence,
      builder: (context, status, _) {
        final online = status == PresenceStatus.online;
        return SwitchListTile(
          key: const Key('presence-toggle'),
          contentPadding: EdgeInsets.zero,
          secondary: Icon(
            online ? Icons.circle : Icons.circle_outlined,
            color: online
                ? presenceOnlineColor
                : Theme.of(context).colorScheme.error,
          ),
          title: const Text('Appear online'),
          subtitle: Text(
            online
                ? 'Others see you as online until you turn this off.'
                : 'Others see you as offline.',
          ),
          value: online,
          onChanged: _presenceBusy ? null : _setOnline,
        );
      },
    ),
    const SizedBox(height: 8),
    Text('Notifications', style: Theme.of(context).textTheme.titleSmall),
    // notification-prefs: Mute the whole app. A single switch; while on, every
    // conversation is silenced, overriding the per-conversation mutes.
    SwitchListTile(
      key: const Key('mute-all-notifications'),
      contentPadding: EdgeInsets.zero,
      secondary: const Icon(Icons.notifications_off_outlined),
      title: const Text('Mute all notifications'),
      subtitle: Text(
        _appMuted
            ? 'Every conversation is silenced, overriding per-chat settings.'
            : 'Silence every conversation at once.',
      ),
      value: _appMuted,
      onChanged: _muteBusy ? null : _setAppMuted,
    ),
    const SizedBox(height: 8),
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
    const SizedBox(height: 8),
    Text('Account', style: Theme.of(context).textTheme.titleSmall),
    // account-deletion: Deleting your account; The admin sees no self-delete.
    // The admin's entry is greyed-out with a note pointing to the operator
    // reset path; an ordinary member gets a working, destructive action.
    if (widget.admin)
      ListTile(
        key: const Key('settings-delete-account'),
        contentPadding: EdgeInsets.zero,
        enabled: false,
        leading: const Icon(Icons.delete_forever_outlined),
        title: const Text('Delete account'),
        subtitle: const Text(
          "The admin account can't be deleted from the app. To reset it, run "
          '`server admin-reset` on the server host (see admin-recovery).',
        ),
      )
    else
      ListTile(
        key: const Key('settings-delete-account'),
        contentPadding: EdgeInsets.zero,
        leading: Icon(
          Icons.delete_forever_outlined,
          color: Theme.of(context).colorScheme.error,
        ),
        title: Text(
          'Delete account',
          style: TextStyle(color: Theme.of(context).colorScheme.error),
        ),
        subtitle: const Text('Permanently delete your account and data.'),
        onTap: user == null ? null : () => _confirmDeleteAccount(user),
      ),
  ];

  /// account-deletion: Deleting your account. Opens the irreversible-delete
  /// confirmation (type-to-confirm the username); on confirm calls the core,
  /// which wipes the device on success (the app then shows onboarding) and
  /// keeps the account on failure.
  Future<void> _confirmDeleteAccount(UserRef user) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => _DeleteAccountDialog(username: user.username),
    );
    if (confirmed != true || !mounted) return;
    try {
      await widget.core.deleteAccount();
      // On success the core moves to the unregistered state and the app shows
      // onboarding; this screen is gone, so there is nothing more to do.
    } catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text(errorText(error))));
      }
    }
  }
}

/// account-deletion: Deleting your account. The irreversible-delete
/// confirmation: it names what is deleted, that it is immediate and cannot be
/// undone, and requires the user to type their own username — the confirm
/// button stays disabled until the typed text matches, so no single tap deletes
/// the account.
class _DeleteAccountDialog extends StatefulWidget {
  const _DeleteAccountDialog({required this.username});

  final String username;

  @override
  State<_DeleteAccountDialog> createState() => _DeleteAccountDialogState();
}

class _DeleteAccountDialogState extends State<_DeleteAccountDialog> {
  final _confirm = TextEditingController();

  bool get _matches => _confirm.text.trim() == widget.username;

  @override
  void dispose() {
    _confirm.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final error = Theme.of(context).colorScheme.error;
    return AlertDialog(
      title: const Text('Delete account'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text(
            'This permanently deletes your account and the data held for you: '
            'your groups and DMs on this device, and your key packages, push '
            'tokens, queued messages and uploaded media on the server. It is '
            'immediate and cannot be undone — there is no deactivation and no '
            'grace period. Messages other people have already received stay on '
            'their devices.',
          ),
          const SizedBox(height: 16),
          Text('Type your username "${widget.username}" to confirm.'),
          const SizedBox(height: 8),
          TextField(
            key: const Key('delete-account-confirm-field'),
            controller: _confirm,
            autofocus: true,
            autocorrect: false,
            enableSuggestions: false,
            decoration: const InputDecoration(
              border: OutlineInputBorder(),
              labelText: 'Username',
            ),
            onChanged: (_) => setState(() {}),
          ),
        ],
      ),
      actions: [
        TextButton(
          key: const Key('delete-account-cancel'),
          onPressed: () => Navigator.of(context).pop(false),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const Key('delete-account-confirm'),
          style: FilledButton.styleFrom(backgroundColor: error),
          onPressed: _matches ? () => Navigator.of(context).pop(true) : null,
          child: const Text('Delete account'),
        ),
      ],
    );
  }
}
