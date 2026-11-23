// app-ui: Named group screen; Admin screens (change `lobby-multi-group`). The
// named group's menu: everyone sees its members; the admin's devices also see
// add member, remove member, rename and close group (the server rejects other
// devices' admin calls anyway).
import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../app/names.dart';
import '../core/core_service.dart';

/// The entries of a named group's menu.
enum GroupMenuEntry {
  members('Members', false),
  addMember('Add member', true),
  removeMember('Remove member', true),
  rename('Rename group', true),
  close('Close group', true);

  const GroupMenuEntry(this.label, this.adminOnly);

  final String label;

  /// Whether only admin devices show it.
  final bool adminOnly;
}

/// The menu of the named group [chatId].
class GroupMenu extends StatelessWidget {
  const GroupMenu({
    super.key,
    required this.core,
    required this.chatId,
    required this.admin,
    required this.names,
  });

  final CoreService core;
  final int chatId;
  final bool admin;
  final UserNames names;

  void _fail(BuildContext context, Object error) {
    ScaffoldMessenger.of(
      context,
    ).showSnackBar(SnackBar(content: Text(errorText(error))));
  }

  /// The users with a device in the group, one entry each.
  Future<List<UserRef>> _members() async {
    final seen = <String>{};
    final users = <UserRef>[];
    for (final d in await core.groupMembers(chatId)) {
      if (seen.add('${d.user.username}@${d.user.serverId}')) users.add(d.user);
    }
    await names.load(users);
    return users;
  }

  Future<void> _show(BuildContext context) async {
    try {
      final users = await _members();
      if (!context.mounted) return;
      await showDialog<void>(
        context: context,
        builder: (dialog) => AlertDialog(
          title: const Text('Members'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              for (final user in users)
                ListTile(
                  key: ValueKey('group-member-${user.username}'),
                  title: Text(names.of(user)),
                ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(dialog).pop(),
              child: const Text('Close'),
            ),
          ],
        ),
      );
    } catch (error) {
      if (context.mounted) _fail(context, error);
    }
  }

  /// Picks one of [users] from a sheet; null if dismissed.
  Future<UserRef?> _pick(BuildContext context, List<UserRef> users) =>
      showModalBottomSheet<UserRef>(
        context: context,
        builder: (sheet) => SafeArea(
          child: ListView(
            shrinkWrap: true,
            children: [
              for (final user in users)
                ListTile(
                  key: ValueKey('pick-${user.username}'),
                  leading: const Icon(Icons.person_outline),
                  title: Text(names.of(user)),
                  onTap: () => Navigator.of(sheet).pop(user),
                ),
            ],
          ),
        ),
      );

  Future<void> _add(BuildContext context) async {
    try {
      final inGroup = {
        for (final u in await _members()) '${u.username}@${u.serverId}',
      };
      final me = core.user.value;
      final everyone = <String, UserRef>{
        for (final d in await core.members())
          '${d.user.username}@${d.user.serverId}': d.user,
      };
      final candidates = [
        for (final e in everyone.entries)
          if (!inGroup.contains(e.key) &&
              (me == null || e.key != '${me.username}@${me.serverId}'))
            e.value,
      ];
      await names.load(candidates);
      if (!context.mounted) return;
      final picked = await _pick(context, candidates);
      if (picked == null) return;
      await core.addToGroup(chatId, picked);
    } catch (error) {
      if (context.mounted) _fail(context, error);
    }
  }

  Future<void> _remove(BuildContext context) async {
    try {
      final me = core.user.value;
      final others = [
        for (final u in await _members())
          if (me == null ||
              u.username != me.username ||
              u.serverId != me.serverId)
            u,
      ];
      if (!context.mounted) return;
      final picked = await _pick(context, others);
      if (picked == null) return;
      await core.removeFromGroup(chatId, picked);
    } catch (error) {
      if (context.mounted) _fail(context, error);
    }
  }

  Future<void> _rename(BuildContext context) async {
    final controller = TextEditingController();
    final name = await showDialog<String>(
      context: context,
      builder: (dialog) => AlertDialog(
        title: const Text('Rename group'),
        content: TextField(
          key: const Key('group-rename-field'),
          controller: controller,
          autofocus: true,
          decoration: const InputDecoration(labelText: 'Group name'),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialog).pop(),
            child: const Text('Cancel'),
          ),
          FilledButton(
            key: const Key('group-rename-save'),
            onPressed: () => Navigator.of(dialog).pop(controller.text.trim()),
            child: const Text('Save'),
          ),
        ],
      ),
    );
    if (name == null || name.isEmpty) return;
    try {
      await core.renameGroup(chatId, name);
    } catch (error) {
      if (context.mounted) _fail(context, error);
    }
  }

  Future<void> _close(BuildContext context) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialog) => AlertDialog(
        title: const Text('Close group?'),
        content: const Text(
          'Everyone keeps the history, but no one can write in it any more.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialog).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            key: const Key('group-close-confirm'),
            onPressed: () => Navigator.of(dialog).pop(true),
            child: const Text('Close group'),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    try {
      await core.closeGroup(chatId);
    } catch (error) {
      if (context.mounted) _fail(context, error);
    }
  }

  @override
  Widget build(BuildContext context) => PopupMenuButton<GroupMenuEntry>(
    key: const Key('group-menu'),
    tooltip: 'Menu',
    onSelected: (entry) => switch (entry) {
      GroupMenuEntry.members => _show(context),
      GroupMenuEntry.addMember => _add(context),
      GroupMenuEntry.removeMember => _remove(context),
      GroupMenuEntry.rename => _rename(context),
      GroupMenuEntry.close => _close(context),
    },
    itemBuilder: (context) => [
      for (final entry in GroupMenuEntry.values)
        if (admin || !entry.adminOnly)
          PopupMenuItem(value: entry, child: Text(entry.label)),
    ],
  );
}
