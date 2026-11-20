// app-ui: Chat list (changes `dms`, `lobby-multi-group`). The lobby first,
// labelled "lobby", then the admin's named groups by name and the DMs by the
// peer's name, newest message first, closed ones last and greyed, each with
// an unread mark and the count of unseen mentions of this user.
//
// app-ui: Create affordance (change `create-affordance`). A `+` button opens a
// menu to start a new conversation: New DM (everyone, when the server offers DMs)
// and New group (admin devices only, when the server offers named groups).
import 'dart:async';

import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../app/names.dart';
import '../admin/create_named_group_screen.dart';
import '../app/shell_app_bar.dart';
import '../core/core_service.dart';
import '../members/members_screen.dart';
import 'chat_model.dart';
import 'chat_screen.dart';
import 'dm_screen.dart';
import 'group_screen.dart';

/// The chat list.
class ChatListScreen extends StatefulWidget {
  const ChatListScreen({
    super.key,
    required this.core,
    required this.chat,
    required this.admin,
  });

  final CoreService core;

  /// The app-level model of the group's chat.
  final ChatModel chat;

  /// Whether this device's user is the admin.
  final bool admin;

  @override
  State<ChatListScreen> createState() => _ChatListScreenState();
}

class _ChatListScreenState extends State<ChatListScreen> {
  late final UserNames _names = UserNames(widget.core);
  List<ChatSummary> _chats = const [];

  /// Whether the server offers DMs: New DM is offered only then (app-ui: Create
  /// affordance).
  bool _dms = false;

  /// Whether the server offers named groups: New group is offered only then.
  bool _groups = false;
  StreamSubscription<CoreEvent>? _subscription;

  @override
  void initState() {
    super.initState();
    _subscription = widget.core.events.listen((event) {
      switch (event) {
        case CoreEvent_ProfileChanged(:final user):
          _names.forget(user);
          _load();
        case CoreEvent_ChatsChanged() ||
            CoreEvent_Message() ||
            CoreEvent_SendStatus():
          _load();
        default:
          break;
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
    final chats = await widget.core.chats();
    final dms = await widget.core.dmsAvailable();
    final groups = await widget.core.namedGroupsAvailable();
    await _names.load([
      for (final c in chats)
        if (c.kind case ChatKindInfo_Dm(:final peer)) peer,
    ]);
    if (mounted) {
      setState(() {
        _chats = chats;
        _dms = dms;
        _groups = groups;
      });
    }
  }

  Future<void> _open(ChatSummary chat) async {
    switch (chat.kind) {
      case ChatKindInfo_Lobby():
        await Navigator.of(context).push(
          MaterialPageRoute<void>(
            builder: (_) => ChatScreen(
              core: widget.core,
              chat: widget.chat,
              admin: widget.admin,
            ),
          ),
        );
      case ChatKindInfo_Group():
        await showGroup(context, widget.core, chat.chatId, admin: widget.admin);
      case ChatKindInfo_Dm(:final peer):
        await showDm(context, widget.core, chat.chatId, peer);
    }
    await _load();
  }

  /// The `+` menu: the actions this device can start (app-ui: Create
  /// affordance). A non-admin on a server without DMs has none, so no button.
  void _showNewMenu() {
    showModalBottomSheet<void>(
      context: context,
      builder: (sheet) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (_dms)
              ListTile(
                key: const Key('new-dm'),
                leading: const Icon(Icons.person_add_alt_1),
                title: const Text('New DM'),
                onTap: () {
                  Navigator.of(sheet).pop();
                  _newDm();
                },
              ),
            if (widget.admin && _groups)
              ListTile(
                key: const Key('new-group'),
                leading: const Icon(Icons.group_add),
                title: const Text('New group'),
                onTap: () {
                  Navigator.of(sheet).pop();
                  _newGroup();
                },
              ),
          ],
        ),
      ),
    );
  }

  /// New DM: pick another member, then open the DM through the core's
  /// `openDm` and show it at once (the same path as the members list's
  /// "Message").
  Future<void> _newDm() async {
    final me = widget.core.user.value;
    final List<MemberUser> members;
    try {
      members = [
        for (final m in byUser(await widget.core.members()))
          if (me == null ||
              m.user.username != me.username ||
              m.user.serverId != me.serverId)
            m,
      ];
    } catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text(errorText(error))));
      }
      return;
    }
    await _names.load(members.map((m) => m.user));
    if (!mounted) return;
    final picked = await showModalBottomSheet<UserRef>(
      context: context,
      builder: (sheet) => SafeArea(
        child: ListView(
          shrinkWrap: true,
          children: [
            for (final m in members)
              ListTile(
                key: ValueKey('pick-${m.user.username}'),
                leading: const Icon(Icons.person_outline),
                title: Text(_names.of(m.user)),
                onTap: () => Navigator.of(sheet).pop(m.user),
              ),
          ],
        ),
      ),
    );
    if (picked == null || !mounted) return;
    await messageUser(context, widget.core, picked);
    await _load();
  }

  /// New group: the admin names it and picks its members; the core creates
  /// a named group and adds them (admin devices only; the server rejects a
  /// non-admin's group RPCs anyway).
  Future<void> _newGroup() async {
    final chatId = await Navigator.of(context).push<int>(
      MaterialPageRoute<int>(
        builder: (_) => CreateNamedGroupScreen(core: widget.core),
      ),
    );
    await _load();
    if (chatId != null && mounted) {
      await showGroup(context, widget.core, chatId, admin: widget.admin);
      await _load();
    }
  }

  Widget? _trailing(ChatSummary chat) {
    final marks = <Widget>[
      if (chat.unseenMentions > 0)
        Badge(
          key: ValueKey('chat-mentions-${chat.chatId}'),
          label: Text('${chat.unseenMentions}'),
          child: const Icon(Icons.alternate_email),
        ),
      if (chat.unread > 0)
        Badge(
          key: ValueKey('chat-unread-${chat.chatId}'),
          label: Text('${chat.unread}'),
        ),
    ];
    if (marks.isEmpty) return null;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        for (final m in marks)
          Padding(padding: const EdgeInsets.only(left: 12), child: m),
      ],
    );
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: widget.chat,
    builder: (context, _) => Scaffold(
      appBar: shellAppBar(
        widget.core,
        title: 'Chats',
        actions: [ChatMenu(admin: widget.admin)],
      ),
      floatingActionButton: (_dms || (widget.admin && _groups))
          ? FloatingActionButton(
              key: const Key('new-chat-fab'),
              tooltip: 'New chat',
              onPressed: _showNewMenu,
              child: const Icon(Icons.add),
            )
          : null,
      // Pull down to retry the connection at once (connect-feedback D2);
      // the core makes it a no-op when already connected.
      body: RefreshIndicator(
        onRefresh: () => widget.core.reconnectNow(),
        child: ListView(
          physics: const AlwaysScrollableScrollPhysics(),
          children: [
            for (final chat in _chats)
              switch (chat.kind) {
                ChatKindInfo_Lobby() => ListTile(
                  key: const ValueKey('chat-main'),
                  leading: const Icon(Icons.groups_outlined),
                  title: const Text(lobbyName),
                  trailing: _trailing(chat),
                  onTap: () => _open(chat),
                ),
                ChatKindInfo_Group(:final name, :final closed) => ListTile(
                  key: ValueKey('chat-${chat.chatId}'),
                  leading: const Icon(Icons.group_outlined),
                  title: Text(
                    name ?? unnamedGroupName,
                    style: closed
                        ? TextStyle(color: Theme.of(context).disabledColor)
                        : null,
                  ),
                  subtitle: closed ? const Text(groupClosedText) : null,
                  trailing: _trailing(chat),
                  onTap: () => _open(chat),
                ),
                ChatKindInfo_Dm(:final peer, :final closed) => ListTile(
                  key: ValueKey('chat-${chat.chatId}'),
                  leading: const Icon(Icons.person_outline),
                  enabled: true,
                  title: Text(
                    _names.of(peer),
                    style: closed
                        ? TextStyle(color: Theme.of(context).disabledColor)
                        : null,
                  ),
                  subtitle: closed
                      ? Text(dmClosedText(_names.short(peer)))
                      : null,
                  trailing: _trailing(chat),
                  onTap: () => _open(chat),
                ),
              },
          ],
        ),
      ),
    ),
  );
}
