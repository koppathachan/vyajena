// app-ui: Profile and members. The group's members by user, each with the
// number of devices they have in the group.
import 'dart:async';

import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../app/moderation.dart';
import '../app/names.dart';
import '../chat/dm_screen.dart';
import '../core/core_service.dart';

/// A member user and their devices in the group.
class MemberUser {
  MemberUser(this.user, this.devices);
  final UserRef user;
  final List<String> devices;
}

/// The group's member devices grouped by user, in the order the core lists
/// them.
List<MemberUser> byUser(List<MemberDevice> devices) {
  final users = <String, MemberUser>{};
  for (final d in devices) {
    final key = '${d.user.username}@${d.user.serverId}';
    users.putIfAbsent(key, () => MemberUser(d.user, [])).devices.add(d.device);
  }
  return users.values.toList();
}

/// "1 device", "2 devices".
String devicesText(int n) => n == 1 ? '1 device' : '$n devices';

/// The members list.
class MembersScreen extends StatefulWidget {
  const MembersScreen({super.key, required this.core});

  final CoreService core;

  @override
  State<MembersScreen> createState() => _MembersScreenState();
}

class _MembersScreenState extends State<MembersScreen> {
  late final UserNames _names = UserNames(widget.core);
  List<MemberUser>? _members;
  String? _error;

  /// Whether the server knows DMs: only then "Message" (app-ui: Open a DM).
  bool _dms = false;

  /// The users blocked on this device, by `username@serverId` (moderation:
  /// Block a user). A blocked member shows a marker and offers Unblock.
  Set<String> _blocked = {};
  StreamSubscription<CoreEvent>? _subscription;

  static String _key(UserRef user) => '${user.username}@${user.serverId}';

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
      final dms = await widget.core.dmsAvailable();
      final blocked = {for (final u in await widget.core.blockedUsers()) _key(u)};
      if (mounted) {
        setState(() {
          _members = members;
          _dms = dms;
          _blocked = blocked;
        });
      }
    } catch (error) {
      if (mounted) setState(() => _error = errorText(error));
    }
  }

  Widget _tile(MemberUser m, bool isMe) {
    final blocked = _blocked.contains(_key(m.user));
    return ListTile(
      leading: const Icon(Icons.person_outline),
      title: Text(isMe ? '${_names.of(m.user)} · you' : _names.of(m.user)),
      subtitle: Text(
        blocked
            ? '${devicesText(m.devices.length)} · blocked'
            : devicesText(m.devices.length),
      ),
      trailing: isMe
          ? null
          : Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (_dms && !blocked)
                  TextButton(
                    key: ValueKey('message-${m.user.username}'),
                    onPressed: () => messageUser(context, widget.core, m.user),
                    child: const Text('Message'),
                  ),
                PopupMenuButton<String>(
                  key: ValueKey('moderate-${m.user.username}'),
                  onSelected: (choice) {
                    switch (choice) {
                      case 'report':
                        _reportUser(m.user);
                      case 'block':
                        _toggleBlock(m.user, true);
                      case 'unblock':
                        _toggleBlock(m.user, false);
                    }
                  },
                  itemBuilder: (context) => [
                    const PopupMenuItem(
                      key: Key('report'),
                      value: 'report',
                      child: Text('Report'),
                    ),
                    if (blocked)
                      const PopupMenuItem(
                        key: Key('unblock'),
                        value: 'unblock',
                        child: Text('Unblock'),
                      )
                    else
                      const PopupMenuItem(
                        key: Key('block'),
                        value: 'block',
                        child: Text('Block'),
                      ),
                  ],
                ),
              ],
            ),
    );
  }

  /// Reports [user] to the admin after asking for a reason (moderation: Report a
  /// user; app-ui: Report a message or a user).
  Future<void> _reportUser(UserRef user) async {
    final messenger = ScaffoldMessenger.of(context);
    final reason = await askReportReason(context);
    if (reason == null) return;
    try {
      await widget.core.reportUser(user, reason);
      messenger.showSnackBar(const SnackBar(content: Text('Report sent')));
    } catch (error) {
      messenger.showSnackBar(SnackBar(content: Text(errorText(error))));
    }
  }

  /// Blocks or unblocks [user] (moderation: Block a user; app-ui: Block and
  /// unblock a user). Blocking confirms it hides them on this device only.
  Future<void> _toggleBlock(UserRef user, bool block) async {
    final messenger = ScaffoldMessenger.of(context);
    final name = _names.of(user);
    if (block && !await confirmBlock(context, name)) return;
    try {
      if (block) {
        await widget.core.blockUser(user);
      } else {
        await widget.core.unblockUser(user);
      }
      await _load();
      messenger.showSnackBar(
        SnackBar(content: Text(block ? 'Blocked $name' : 'Unblocked $name')),
      );
    } catch (error) {
      messenger.showSnackBar(SnackBar(content: Text(errorText(error))));
    }
  }

  @override
  Widget build(BuildContext context) {
    final members = _members;
    final me = widget.core.user.value;
    return Scaffold(
      appBar: AppBar(title: const Text('Members')),
      body: switch ((members, _error)) {
        (_, final String error) => Center(child: Text(error)),
        (null, _) => const SizedBox.shrink(),
        (final List<MemberUser> members, _) => ListView(
          children: [
            for (final m in members)
              _tile(
                m,
                me != null &&
                    me.username == m.user.username &&
                    me.serverId == m.user.serverId,
              ),
          ],
        ),
      },
    );
  }
}
