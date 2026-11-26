// app-ui: Join from an invite. A new user pastes the invite the admin
// shared, sees the user and server it names, and joins; a device of an
// existing user is linked instead (app-ui: Link a device).
import 'package:flutter/material.dart';

import '../app/arcade/motion.dart';
import '../app/errors.dart';
import '../app/routes.dart';
import '../app/shell_app_bar.dart';
import '../core/core_service.dart';

/// Join from an invite, or link this device to an existing account.
class OnboardingScreen extends StatefulWidget {
  const OnboardingScreen({super.key, required this.core});

  final CoreService core;

  @override
  State<OnboardingScreen> createState() => _OnboardingScreenState();
}

class _OnboardingScreenState extends State<OnboardingScreen> {
  final _invite = TextEditingController();
  final _username = TextEditingController();
  final _joinCode = TextEditingController();
  InvitePreview? _preview;
  // dev-onboarding: the server's advertised onboarding, once checked. When it
  // reports open mode, the simplified username screen is shown instead of the
  // invite preview (app-ui: Simplified onboarding in open mode).
  ServerOnboarding? _onboarding;
  String? _error;
  bool _joining = false;
  bool _checking = false;

  @override
  void dispose() {
    _invite.dispose();
    _username.dispose();
    _joinCode.dispose();
    super.dispose();
  }

  void _reset() {
    if (_preview != null || _onboarding != null || _error != null) {
      setState(() {
        _preview = null;
        _onboarding = null;
        _error = null;
      });
    }
  }

  Future<void> _check() async {
    final text = _invite.text.trim();
    setState(() {
      _preview = null;
      _onboarding = null;
      _error = null;
      _checking = true;
    });
    try {
      // Offline validation + preview (app-ui: Join from an invite).
      final preview = widget.core.parseInvite(text);
      // dev-onboarding: probe the server so an open/dev server can offer the
      // simplified username screen. A failure here (e.g. the server is
      // unreachable) just falls back to the ordinary invite flow.
      ServerOnboarding? onboarding;
      try {
        onboarding = await widget.core.serverOnboarding(text);
      } catch (_) {
        onboarding = null;
      }
      if (!mounted) return;
      setState(() {
        _checking = false;
        _preview = preview;
        _onboarding = onboarding;
      });
    } on CoreError catch (error) {
      if (!mounted) return;
      setState(() {
        _checking = false;
        _error = 'This is not a valid invite: ${error.message}';
      });
    }
  }

  Future<void> _join() async {
    setState(() {
      _joining = true;
      _error = null;
    });
    try {
      await widget.core.register(_invite.text.trim());
      // The core's state turns ready and the home screen shows the group.
    } catch (error) {
      _showJoinError(error);
    }
  }

  // dev-onboarding: open registration with a chosen username (app-ui:
  // Simplified onboarding in open mode).
  Future<void> _joinOpen(String username) async {
    if (username.isEmpty) {
      setState(() => _error = 'Choose a username to join.');
      return;
    }
    setState(() {
      _joining = true;
      _error = null;
    });
    try {
      await widget.core.registerOpen(
        _invite.text.trim(),
        username,
        _joinCode.text.trim(),
      );
    } catch (error) {
      _showJoinError(error);
    }
  }

  void _showJoinError(Object error) {
    if (!mounted) return;
    setState(() {
      _joining = false;
      // A network error or an expired invite speak for themselves; anything
      // else is prefixed so the user knows the join did not go through
      // (app-ui: Invite expiry is surfaced).
      _error =
          error is CoreError &&
              (error.kind == CoreErrorKind.network ||
                  error.kind == CoreErrorKind.inviteExpired)
          ? errorText(error)
          : 'Could not join. ${errorText(error)}';
    });
  }

  @override
  Widget build(BuildContext context) {
    final preview = _preview;
    final onboarding = _onboarding;
    final text = Theme.of(context).textTheme;
    return Scaffold(
      appBar: shellAppBar(widget.core),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: arcadeEntrances([
          Text('Join the group', style: text.titleLarge),
          const SizedBox(height: 8),
          const Text('Paste the invite the admin shared with you.'),
          const SizedBox(height: 12),
          TextField(
            key: const Key('invite'),
            controller: _invite,
            minLines: 2,
            maxLines: 4,
            // An invite is a case-sensitive base64url payload after a fixed
            // prefix: the keyboard must not capitalize or "correct" it, or a
            // typed invite fails to parse (the core also strips the prefix
            // case-insensitively as a backstop).
            autocorrect: false,
            enableSuggestions: false,
            textCapitalization: TextCapitalization.none,
            decoration: const InputDecoration(
              border: OutlineInputBorder(),
              labelText: 'Invite',
            ),
            onChanged: (_) => _reset(),
          ),
          const SizedBox(height: 12),
          if (preview == null)
            Align(
              alignment: Alignment.centerRight,
              child: FilledButton(
                onPressed: _checking ? null : _check,
                child: Text(_checking ? 'Checking…' : 'Check invite'),
              ),
            )
          else if (onboarding != null && onboarding.open) ...[
            // dev-onboarding: open mode — pick a username, no invite needed
            // (app-ui: Simplified onboarding in open mode).
            Card.outlined(
              child: ListTile(
                leading: const Icon(Icons.dns_outlined),
                title: Text('Open sign-up on ${onboarding.serverId}'),
                subtitle: const Text('Pick a username to join — no invite needed.'),
              ),
            ),
            const SizedBox(height: 12),
            TextField(
              key: const Key('username'),
              controller: _username,
              autocorrect: false,
              enableSuggestions: false,
              textCapitalization: TextCapitalization.none,
              onChanged: (_) => setState(() {}),
              decoration: const InputDecoration(
                border: OutlineInputBorder(),
                labelText: 'Username',
              ),
            ),
            if (onboarding.joinCodeRequired) ...[
              const SizedBox(height: 12),
              TextField(
                key: const Key('joinCode'),
                controller: _joinCode,
                autocorrect: false,
                enableSuggestions: false,
                textCapitalization: TextCapitalization.none,
                decoration: const InputDecoration(
                  border: OutlineInputBorder(),
                  labelText: 'Join code',
                ),
              ),
            ],
            const SizedBox(height: 12),
            Wrap(
              alignment: WrapAlignment.end,
              spacing: 8,
              runSpacing: 8,
              children: [
                OutlinedButton(
                  key: const Key('joinAsAdmin'),
                  onPressed: _joining
                      ? null
                      : () => _joinOpen(onboarding.adminUsername),
                  child: Text('Join as the admin (${onboarding.adminUsername})'),
                ),
                FilledButton(
                  onPressed: _joining
                      ? null
                      : () => _joinOpen(_username.text.trim()),
                  child: Text(_joining ? 'Joining…' : 'Join'),
                ),
              ],
            ),
          ] else ...[
            Card.outlined(
              child: ListTile(
                leading: const Icon(Icons.person_outline),
                title: Text(userText(preview.user)),
                subtitle: Text('on ${preview.relayAddress}'),
              ),
            ),
            const SizedBox(height: 12),
            Align(
              alignment: Alignment.centerRight,
              child: FilledButton(
                onPressed: _joining ? null : _join,
                child: Text(_joining ? 'Joining…' : 'Join'),
              ),
            ),
          ],
          if (_error case final error?) ...[
            const SizedBox(height: 12),
            Text(
              error,
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
          ],
          const SizedBox(height: 32),
          const Divider(),
          TextButton.icon(
            onPressed: () =>
                Navigator.of(context).pushNamed(Routes.linkThisDevice),
            icon: const Icon(Icons.devices_outlined),
            label: const Text('I already have an account: link this device'),
          ),
        ]),
      ),
    );
  }
}
