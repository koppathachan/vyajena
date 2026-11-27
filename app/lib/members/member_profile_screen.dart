// app-shell: Member profile (change `shell-nav-consistency`). Tapping another
// member on the Contacts tab opens this: their avatar, name and device count, a
// Message button that opens or starts the DM (or says DMs are not offered yet), and
// the report/block affordances (change `report-and-block`). It replaces the
// per-member row actions of the retired members list.
import 'dart:async';

import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../app/moderation.dart';
import '../app/names.dart';
import '../chat/dm_screen.dart';
import '../core/core_service.dart';
import '../shell/avatars.dart';
import 'contacts_tab.dart' show dmsUnavailableText;
import 'member.dart';

/// The profile of another member, reached by tapping them on the Contacts tab.
class MemberProfileScreen extends StatefulWidget {
  const MemberProfileScreen({
    super.key,
    required this.core,
    required this.member,
  });

  final CoreService core;

  /// The member and their devices in the group.
  final MemberUser member;

  @override
  State<MemberProfileScreen> createState() => _MemberProfileScreenState();
}

class _MemberProfileScreenState extends State<MemberProfileScreen> {
  late final UserNames _names = UserNames(widget.core);
  UserRef get _user => widget.member.user;

  static String _key(UserRef user) => '${user.username}@${user.serverId}';

  bool _loaded = false;

  /// Whether the server knows DMs: only then "Message" opens one, else it says
  /// so (app-ui: Contact tap).
  bool _dms = false;

  /// Whether this user is blocked on this device (moderation: Block a user).
  bool _blocked = false;
  StreamSubscription<CoreEvent>? _subscription;

  @override
  void initState() {
    super.initState();
    _subscription = widget.core.events.listen((event) {
      if (event case CoreEvent_ProfileChanged(:final user)) {
        if (_key(user) == _key(_user)) {
          _names.forget(user);
          _load();
        }
      }
    });
    // DM availability is learnt when the device connects, which may be after this
    // screen opened: look again on every change.
    widget.core.connection.addListener(_load);
    _load();
  }

  @override
  void dispose() {
    widget.core.connection.removeListener(_load);
    _subscription?.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    try {
      await _names.load([_user]);
      final dms = await widget.core.dmsAvailable();
      final blocked = {
        for (final u in await widget.core.blockedUsers()) _key(u),
      }.contains(_key(_user));
      if (mounted) {
        setState(() {
          _dms = dms;
          _blocked = blocked;
          _loaded = true;
        });
      }
    } catch (error) {
      if (mounted) {
        setState(() => _loaded = true);
        _tell(errorText(error));
      }
    }
  }

  void _tell(String text) => ScaffoldMessenger.of(context)
    ..hideCurrentSnackBar()
    ..showSnackBar(SnackBar(content: Text(text)));

  /// Opens or starts the DM with this member (app-ui: Contact tap): DM
  /// availability is asked afresh, and when DMs are not offered a snack bar says
  /// why nothing opens, so the button never does nothing.
  Future<void> _message() async {
    final dms = await widget.core.dmsAvailable();
    if (!mounted) return;
    if (dms != _dms) setState(() => _dms = dms);
    if (dms) {
      await messageUser(context, widget.core, _user);
    } else {
      _tell(dmsUnavailableText);
    }
  }

  /// Reports this member to the admin after asking for a reason (moderation:
  /// Report a user; app-ui: Report a message or a user).
  Future<void> _report() async {
    final messenger = ScaffoldMessenger.of(context);
    final reason = await askReportReason(context);
    if (reason == null) return;
    try {
      await widget.core.reportUser(_user, reason);
      messenger.showSnackBar(const SnackBar(content: Text('Report sent')));
    } catch (error) {
      messenger.showSnackBar(SnackBar(content: Text(errorText(error))));
    }
  }

  /// Blocks or unblocks this member (moderation: Block a user; app-ui: Block and
  /// unblock a user). Blocking confirms it hides them on this device only.
  Future<void> _toggleBlock(bool block) async {
    final messenger = ScaffoldMessenger.of(context);
    final name = _names.of(_user);
    if (block && !await confirmBlock(context, name)) return;
    try {
      if (block) {
        await widget.core.blockUser(_user);
      } else {
        await widget.core.unblockUser(_user);
      }
      if (mounted) setState(() => _blocked = block);
      messenger.showSnackBar(
        SnackBar(content: Text(block ? 'Blocked $name' : 'Unblocked $name')),
      );
    } catch (error) {
      messenger.showSnackBar(SnackBar(content: Text(errorText(error))));
    }
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final name = _names.of(_user);
    final devices = devicesText(widget.member.devices.length);
    return Scaffold(
      appBar: AppBar(title: const Text('Profile')),
      body: ListView(
        padding: const EdgeInsets.all(24),
        children: [
          Center(
            child: PresenceAvatar(
              key: const Key('member-avatar'),
              core: widget.core,
              user: _user,
              name: _loaded ? _names.short(_user) : _user.username,
              seed: _user.username,
              radius: 44,
            ),
          ),
          const SizedBox(height: 16),
          Text(
            _loaded ? name : _user.username,
            key: const Key('member-name'),
            textAlign: TextAlign.center,
            style: Theme.of(context).textTheme.titleLarge,
          ),
          const SizedBox(height: 4),
          Text(
            _blocked ? '$devices · blocked' : devices,
            key: const Key('member-devices'),
            textAlign: TextAlign.center,
            style: Theme.of(
              context,
            ).textTheme.bodyMedium?.copyWith(color: scheme.onSurfaceVariant),
          ),
          const SizedBox(height: 32),
          FilledButton.icon(
            key: ValueKey('message-${_user.username}'),
            onPressed: _blocked ? null : _message,
            icon: const Icon(Icons.chat_bubble_outline),
            label: const Text('Message'),
          ),
          const SizedBox(height: 8),
          OutlinedButton.icon(
            key: ValueKey('report-${_user.username}'),
            onPressed: _report,
            icon: const Icon(Icons.flag_outlined),
            label: const Text('Report'),
          ),
          const SizedBox(height: 8),
          if (_blocked)
            OutlinedButton.icon(
              key: ValueKey('unblock-${_user.username}'),
              onPressed: () => _toggleBlock(false),
              icon: const Icon(Icons.block),
              label: const Text('Unblock'),
            )
          else
            OutlinedButton.icon(
              key: ValueKey('block-${_user.username}'),
              onPressed: () => _toggleBlock(true),
              icon: const Icon(Icons.block),
              label: Text(
                'Block',
                style: TextStyle(color: scheme.error),
              ),
            ),
        ],
      ),
    );
  }
}
