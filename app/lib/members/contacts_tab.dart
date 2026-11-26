// app-shell: Contacts tab. The server's members, each with an avatar and name,
// an `admin` badge and a "you" marker; tapping a member opens or starts the DM
// with them (app-ui: Contact tap: tapping yourself opens Profile, and a tap
// when DMs are not offered says so, so a tap never does nothing). The admin's device also offers Invite, New group and the other
// admin actions; no other device shows any of them.
import 'dart:async';

import 'package:flutter/material.dart';

import '../admin/create_named_group_screen.dart';
import '../app/arcade/arcade_border.dart';
import '../app/arcade/motion.dart';
import '../app/arcade/sketch.dart';
import '../app/errors.dart';
import '../app/names.dart';
import '../app/routes.dart';
import '../app/shell_app_bar.dart';
import '../chat/dm_screen.dart';
import '../chat/group_screen.dart';
import '../core/core_service.dart';
import '../shell/avatars.dart';
import 'members_screen.dart';

/// What a contact tap says when the server does not offer DMs (yet).
const dmsUnavailableText =
    'Direct messages are not available yet. Try again once connected.';

/// The Contacts tab.
class ContactsTab extends StatefulWidget {
  const ContactsTab({
    super.key,
    required this.core,
    required this.admin,
    this.onOpenProfile,
  });

  final CoreService core;

  /// Whether this device's user is the admin.
  final bool admin;

  /// Opens the Profile tab (tapping your own row).
  final VoidCallback? onOpenProfile;

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
    // Whether the server offers DMs is learnt when the device connects, which
    // may be after this tab first loaded: look again on every change.
    widget.core.connection.addListener(_load);
    _load();
  }

  @override
  void dispose() {
    widget.core.connection.removeListener(_load);
    _subscription?.cancel();
    super.dispose();
  }

  /// A tap on a member always answers: your own row opens Profile, another
  /// member's opens the DM (DM availability asked afresh), and when DMs are
  /// not offered a snack bar says why nothing opens.
  Future<void> _tapMember(MemberUser m) async {
    if (_isMe(m.user)) {
      final open = widget.onOpenProfile;
      if (open != null) {
        open();
      } else {
        _tell('This is you.');
      }
      return;
    }
    final dms = await widget.core.dmsAvailable();
    if (!mounted) return;
    if (dms != _dms) setState(() => _dms = dms);
    if (dms) {
      await messageUser(context, widget.core, m.user);
    } else {
      _tell(dmsUnavailableText);
    }
  }

  void _tell(String text) => ScaffoldMessenger.of(context)
    ..hideCurrentSnackBar()
    ..showSnackBar(SnackBar(content: Text(text)));

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
        leading: ArcadeFloat(
          distance: 3,
          phase: floatPhase(key),
          child: Askew(
            seed: sketchSeed(key),
            degrees: 3,
            child: Container(
              width: 40,
              height: 40,
              decoration: sketchBox(
                fill: Theme.of(context).colorScheme.primary,
                border: Theme.of(context).colorScheme.outline,
                seed: sketchSeed(key),
              ),
              child: Icon(icon, color: Theme.of(context).colorScheme.onPrimary),
            ),
          ),
        ),
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
      // manual-presence: Presence indicator on avatars.
      // Arcade motion: each contact's icon bobs on its own beat, askew.
      leading: ArcadeFloat(
        distance: 3,
        phase: floatPhase('contact-${m.user.username}'),
        child: Askew(
          seed: sketchSeed(m.user.username),
          degrees: 3,
          child: PresenceAvatar(
            core: widget.core,
            user: m.user,
            name: _names.short(m.user),
            seed: m.user.username,
          ),
        ),
      ),
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
              decoration: sketchBox(
                fill: scheme.primaryContainer,
                border: scheme.outline,
                width: 1.5,
                seed: sketchSeed('admin-${m.user.username}'),
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
      onTap: () => _tapMember(m),
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
          children: arcadeEntrances([
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
              const SketchDivider(seed: 5),
            ],
            for (final m in members) _member(m),
          ]),
        ),
      },
    );
  }
}
