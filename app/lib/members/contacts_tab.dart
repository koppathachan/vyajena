// app-shell: Contacts tab. The server's members, each with an avatar and name,
// an `admin` badge and a "you" marker; tapping a member opens or starts the DM
// with them. The admin's device also offers Invite, New group and the other
// admin actions; no other device shows any of them.
import 'dart:async';

import 'package:flutter/material.dart';

import '../admin/create_named_group_screen.dart';
import '../app/errors.dart';
import '../app/names.dart';
import '../app/routes.dart';
import '../app/shell_app_bar.dart';
import '../chat/dm_screen.dart';
import '../chat/group_screen.dart';
import '../core/core_service.dart';
import '../shell/avatars.dart';
import 'members_screen.dart';

/// The Contacts tab.
class ContactsTab extends StatefulWidget {
  const ContactsTab({super.key, required this.core, required this.admin});

  final CoreService core;

  /// Whether this device's user is the admin.
  final bool admin;

  @override
  State<ContactsTab> createState() => _ContactsTabState();
}

class _ContactsTabState extends State<ContactsTab> {
  late final UserNames _names = UserNames(widget.core);
  List<MemberUser>? _members;
  UserRef? _admin;
  String? _error;
  bool _dms = false;
  bool _groups = false;
  StreamSubscription<CoreEvent>? _subscription;

  @override
  void initState() {
    super.initState();
    _subscription = widget.core.events.listen((event) {
      if (event case CoreEvent_ProfileChanged(:final user)) {
        _names.forget(user);
        _load();
      } else if (event is CoreEvent_MembersChanged) {
        _load();
      }
    });
    _load();
  }

  @override
  void dispose() {
    _subscription?.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    try {
      final members = byUser(await widget.core.members());
      await _names.load(members.map((m) => m.user));
      final adminUser = await widget.core.adminUser();
      final dms = await widget.core.dmsAvailable();
      final groups = await widget.core.namedGroupsAvailable();
      if (mounted) {
        setState(() {
          _members = members;
          _admin = adminUser;
          _dms = dms;
          _groups = groups;
          _error = null;
        });
      }
    } catch (error) {
      if (mounted) setState(() => _error = errorText(error));
    }
  }

  bool _isMe(UserRef user) {
    final me = widget.core.user.value;
    return me != null &&
        me.username == user.username &&
        me.serverId == user.serverId;
  }

  Future<void> _newGroup() async {
    final chatId = await Navigator.of(context).push<int>(
      MaterialPageRoute<int>(
        builder: (_) => CreateNamedGroupScreen(core: widget.core),
      ),
    );
    if (chatId != null && mounted) {
      await showGroup(context, widget.core, chatId, admin: widget.admin);
    }
  }

  Widget _action(String key, IconData icon, String label, VoidCallback onTap) =>
      ListTile(
        key: Key(key),
        leading: CircleAvatar(child: Icon(icon)),
        title: Text(label),
        onTap: onTap,
      );

  Widget _member(MemberUser m) {
    final me = _isMe(m.user);
    final scheme = Theme.of(context).colorScheme;
    final name = _names.of(m.user);
    // The group names its admin to every member (core.adminUser); this
    // device's own admin status covers the moment before it has joined.
    final named = _admin;
    final admin =
        (named != null &&
            named.username == m.user.username &&
            named.serverId == m.user.serverId) ||
        (me && widget.admin);
    return ListTile(
      key: ValueKey('contact-${m.user.username}'),
      leading: NameAvatar(name: _names.short(m.user), seed: m.user.username),
      title: Row(
        children: [
          Flexible(child: Text(name, overflow: TextOverflow.ellipsis)),
          if (me) ...[
            const SizedBox(width: 8),
            Text(
              'you',
              key: ValueKey('you-${m.user.username}'),
              style: Theme.of(
                context,
              ).textTheme.labelMedium?.copyWith(color: scheme.onSurfaceVariant),
            ),
          ],
          if (admin) ...[
            const SizedBox(width: 8),
            Container(
              key: ValueKey('admin-badge-${m.user.username}'),
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
              decoration: BoxDecoration(
                color: scheme.primaryContainer,
                borderRadius: BorderRadius.circular(8),
              ),
              child: Text(
                'admin',
                style: Theme.of(context).textTheme.labelSmall?.copyWith(
                  color: scheme.onPrimaryContainer,
                ),
              ),
            ),
          ],
        ],
      ),
      subtitle: Text(devicesText(m.devices.length)),
      onTap: _dms && !me
          ? () => messageUser(context, widget.core, m.user)
          : null,
    );
  }

  @override
  Widget build(BuildContext context) {
    final members = _members;
    return Scaffold(
      appBar: shellAppBar(widget.core, title: 'Contacts'),
      body: switch ((members, _error)) {
        (_, final String error) => Center(child: Text(error)),
        (null, _) => const SizedBox.shrink(),
        (final List<MemberUser> members, _) => ListView(
          children: [
            // TODO(replace-user): the admin's "Replace user" entry goes here
            // once that change ships; it is not offered before then.
            if (widget.admin) ...[
              _action(
                'contacts-invite',
                Icons.person_add_alt_1,
                'Invite a user',
                () => Navigator.of(context).pushNamed(Routes.invite),
              ),
              _action(
                'contacts-invites',
                Icons.mail_outline,
                'Invites',
                () => Navigator.of(context).pushNamed(Routes.invites),
              ),
              if (_groups)
                _action(
                  'contacts-new-group',
                  Icons.group_add,
                  'New group',
                  _newGroup,
                ),
              _action(
                'contacts-remove-user',
                Icons.person_remove_outlined,
                'Remove a user',
                () => Navigator.of(context).pushNamed(Routes.removeUser),
              ),
              const Divider(),
            ],
            for (final m in members) _member(m),
          ],
        ),
      },
    );
  }
}
