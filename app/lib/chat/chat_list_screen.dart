// app-shell: Chats list (the Chats tab); app-ui: Chat list (changes `dms`,
// `lobby-multi-group`). One unified list: the lobby first, labelled "lobby",
// then the admin's named groups, then the DMs by the peer's name, newest
// message first, closed ones last and greyed. Each row: an avatar, the name, a
// one-line preview of the newest message (with a media glyph), its time, an
// unread count and the count of unseen mentions of this user.
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
import '../shell/avatars.dart';
import '../core/core_service.dart';
import '../members/members_screen.dart';
import 'chat_model.dart';
import 'chat_preview.dart';
import 'chat_screen.dart';
import 'dm_screen.dart';
import 'group_screen.dart';
import 'time_text.dart';

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

  /// The preview of each chat's newest message, by chat id.
  Map<int, ChatPreview> _previews = const {};

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
    final chats = _arranged(await widget.core.chats());
    final dms = await widget.core.dmsAvailable();
    final groups = await widget.core.namedGroupsAvailable();
    await _names.load([
      for (final c in chats)
        if (c.kind case ChatKindInfo_Dm(:final peer)) peer,
    ]);
    final previews = <int, ChatPreview>{};
    for (final c in chats) {
      final newest = (await widget.core.historyIn(c.chatId, 1)).lastOrNull;
      if (newest == null) continue;
      if (c.kind is ChatKindInfo_Group || c.kind is ChatKindInfo_Lobby) {
        await _names.load([newest.sender]);
      }
      previews[c.chatId] = previewOf(
        newest,
        from: c.kind is ChatKindInfo_Dm ? null : _names.short(newest.sender),
      );
    }
    if (mounted) {
      setState(() {
        _chats = chats;
        _previews = previews;
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

  /// The lobby, then open named groups, then open DMs (each in the order the
  /// core gives, newest first), then the closed ones (app-shell: Chats list).
  static List<ChatSummary> _arranged(List<ChatSummary> chats) {
    bool closed(ChatSummary c) => switch (c.kind) {
      ChatKindInfo_Lobby() => false,
      ChatKindInfo_Group(:final closed) => closed,
      ChatKindInfo_Dm(:final closed) => closed,
    };
    int rank(ChatSummary c) => switch (c.kind) {
      ChatKindInfo_Lobby() => 0,
      ChatKindInfo_Group() => closed(c) ? 3 : 1,
      ChatKindInfo_Dm() => closed(c) ? 4 : 2,
    };
    final indexed = [for (var i = 0; i < chats.length; i++) (i, chats[i])];
    indexed.sort((a, b) {
      final byRank = rank(a.$2).compareTo(rank(b.$2));
      return byRank != 0 ? byRank : a.$1.compareTo(b.$1);
    });
    return [for (final e in indexed) e.$2];
  }

  Widget? _trailing(ChatSummary chat) {
    final scheme = Theme.of(context).colorScheme;
    final at = chat.lastMessageAtMs;
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
    if (marks.isEmpty && at == null) return null;
    return Column(
      mainAxisAlignment: MainAxisAlignment.center,
      crossAxisAlignment: CrossAxisAlignment.end,
      mainAxisSize: MainAxisSize.min,
      children: [
        if (at != null)
          Text(
            listTimeText(context, at),
            key: ValueKey('chat-time-${chat.chatId}'),
            style: Theme.of(context).textTheme.labelSmall?.copyWith(
              color: chat.unread > 0 ? scheme.primary : scheme.onSurfaceVariant,
            ),
          ),
        if (marks.isNotEmpty) ...[
          const SizedBox(height: 4),
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              for (final m in marks)
                Padding(padding: const EdgeInsets.only(left: 8), child: m),
            ],
          ),
        ],
      ],
    );
  }

  /// The row's second line: the closed notice, else the newest message's
  /// preview with its media glyph, else nothing.
  Widget? _subtitle(ChatSummary chat, String? closedText) {
    if (closedText != null) return Text(closedText);
    final preview = _previews[chat.chatId];
    if (preview == null) return null;
    final muted = Theme.of(context).colorScheme.onSurfaceVariant;
    return Row(
      children: [
        if (preview.icon != null) ...[
          Icon(preview.icon, size: 16, color: muted),
          const SizedBox(width: 4),
        ],
        Expanded(
          child: Text(
            preview.text,
            key: ValueKey('chat-preview-${chat.chatId}'),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
        ),
      ],
    );
  }

  /// One row. Closed chats are greyed; a closed DM can be swiped away.
  Widget _row(ChatSummary chat) {
    final disabled = Theme.of(context).disabledColor;
    final (
      Key key,
      String title,
      String? closedText,
      bool closed,
      Widget avatar,
    ) = switch (chat.kind) {
      ChatKindInfo_Lobby() => (
        const ValueKey('chat-main'),
        lobbyName,
        null,
        false,
        const NameAvatar(name: lobbyName, icon: Icons.groups_outlined),
      ),
      ChatKindInfo_Group(:final name, :final closed) => (
        ValueKey('chat-${chat.chatId}'),
        name ?? unnamedGroupName,
        closed ? groupClosedText : null,
        closed,
        NameAvatar(
          name: name ?? unnamedGroupName,
          seed: 'group-${chat.chatId}',
          muted: closed,
        ),
      ),
      ChatKindInfo_Dm(:final peer, :final closed) => (
        ValueKey('chat-${chat.chatId}'),
        _names.of(peer),
        closed ? dmClosedText(_names.short(peer)) : null,
        closed,
        NameAvatar(
          name: _names.short(peer),
          seed: peer.username,
          muted: closed,
        ),
      ),
    };
    final tile = ListTile(
      key: key,
      leading: avatar,
      enabled: true,
      title: Text(
        title,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: closed ? TextStyle(color: disabled) : null,
      ),
      subtitle: _subtitle(chat, closedText),
      trailing: _trailing(chat),
      onTap: () => _open(chat),
    );
    if (!(closed && chat.kind is ChatKindInfo_Dm)) return tile;
    // TODO(delete-messages): the swipe removes a closed DM through
    // `Dms.RemoveClosedDm` (message-deletion: Remove a closed DM). That RPC
    // is not built yet, so the swipe only shows the affordance and says so;
    // it never hides or deletes anything.
    return Dismissible(
      key: ValueKey('dismiss-${chat.chatId}'),
      direction: DismissDirection.endToStart,
      background: Container(
        color: Theme.of(context).colorScheme.errorContainer,
        alignment: Alignment.centerRight,
        padding: const EdgeInsets.symmetric(horizontal: 24),
        child: Icon(
          Icons.delete_outline,
          color: Theme.of(context).colorScheme.onErrorContainer,
        ),
      ),
      confirmDismiss: (_) async {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(const SnackBar(content: Text(removeClosedDmSoonText)));
        return false;
      },
      child: tile,
    );
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: widget.chat,
    builder: (context, _) => Scaffold(
      appBar: shellAppBar(widget.core, title: 'Chats'),
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
          children: [for (final chat in _chats) _row(chat)],
        ),
      ),
    ),
  );
}

/// What the swipe on a closed DM says until `delete-messages` ships.
const removeClosedDmSoonText = 'Removing a closed chat is coming soon.';
