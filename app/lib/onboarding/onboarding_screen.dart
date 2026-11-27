// app-ui: Join from an invite. A new user pastes the invite the admin
// shared, sees the user and server it names, and joins; a device of an
// existing user is linked instead (app-ui: Link a device).
import 'package:flutter/material.dart';

import '../app/arcade/sketch.dart';

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
  InvitePreview? _preview;
  String? _error;
  bool _joining = false;

  @override
  void dispose() {
    _invite.dispose();
    super.dispose();
  }

  void _check() {
    setState(() {
      _preview = null;
      _error = null;
      try {
        _preview = widget.core.parseInvite(_invite.text.trim());
      } on CoreError catch (error) {
        _error = 'This is not a valid invite: ${error.message}';
      }
    });
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
  }

  @override
  Widget build(BuildContext context) {
    final preview = _preview;
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
            onChanged: (_) {
              if (_preview != null || _error != null) {
                setState(() {
                  _preview = null;
                  _error = null;
                });
              }
            },
          ),
          const SizedBox(height: 12),
          if (preview == null)
            Align(
              alignment: Alignment.centerRight,
              child: FilledButton(
                onPressed: _check,
                child: const Text('Check invite'),
              ),
            )
          else ...[
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
          const SketchDivider(seed: 2),
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
