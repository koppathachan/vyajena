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

  /// Whether whole-app mute is on (notification-prefs: Mute the whole app):
  /// shown as a banner, and it overrides the per-row mute markers.
  bool _appMuted = false;
  StreamSubscription<CoreEvent>? _subscription;

  @override
  void initState() {
    super.initState();
    _subscription = widget.core.events.listen((event) {
      switch (event) {
        case CoreEvent_ProfileChanged(:final user):
          _names.forget(user);
          _reload();
        case CoreEvent_ChatsChanged() ||
            CoreEvent_Message() ||
            CoreEvent_SendStatus() ||
            CoreEvent_MutePrefsChanged():
          _reload();
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

  bool _loading = false;
  bool _again = false;

  /// [_load], coalesced: a burst of events (a message, its send status, the
  /// chats change) runs one load, plus at most one more after it for what
  /// arrived meanwhile, instead of one full load per event.
  Future<void> _reload() async {
    if (_loading) {
      _again = true;
      return;
    }
    _loading = true;
    try {
      do {
        _again = false;
        await _load();
      } while (_again && mounted);
    } finally {
      _loading = false;
    }
  }

  Future<void> _load() async {
    final chats = _arranged(await widget.core.chats());
    final (dms, groups, appMuted) = await (
      widget.core.dmsAvailable(),
      widget.core.namedGroupsAvailable(),
      widget.core.appMuted(),
    ).wait;
    await _names.load([
      for (final c in chats)
        if (c.kind case ChatKindInfo_Dm(:final peer)) peer,
    ]);
    // The newest message of every chat, fetched in parallel.
    final newest = await Future.wait([
      for (final c in chats) widget.core.historyIn(c.chatId, 1),
    ]);
    await _names.load([
      for (var i = 0; i < chats.length; i++)
        if (chats[i].kind is! ChatKindInfo_Dm && newest[i].isNotEmpty)
          newest[i].last.sender,
    ]);
    final previews = <int, ChatPreview>{};
    for (var i = 0; i < chats.length; i++) {
      final message = newest[i].lastOrNull;
      if (message == null) continue;
      previews[chats[i].chatId] = previewOf(
        message,
        from: chats[i].kind is ChatKindInfo_Dm
            ? null
            : _names.short(message.sender),
      );
    }
    if (mounted) {
      setState(() {
        _chats = chats;
        _previews = previews;
        _dms = dms;
        _groups = groups;
        _appMuted = appMuted;
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
    // notification-prefs: Mute a conversation. A muted row shows a bell-off
    // marker, unless whole-app mute is on (then the banner conveys it, per-row
    // markers would be misleading).
    final showMuteMarker = chat.muted && !_appMuted;
    final titleText = Text(
      title,
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
      style: closed ? TextStyle(color: disabled) : null,
    );
    final tile = ListTile(
      key: key,
      leading: avatar,
      enabled: true,
      // Keep a plain Text title when there is no marker, so the row reads the
      // same as before (tests and layout).
      title: showMuteMarker
          ? Row(
              children: [
                Icon(
                  Icons.notifications_off_outlined,
                  key: ValueKey('chat-muted-${chat.chatId}'),
                  size: 16,
                  color: disabled,
                ),
                const SizedBox(width: 4),
                Flexible(child: titleText),
              ],
            )
          : titleText,
      subtitle: _subtitle(chat, closedText),
      trailing: _trailing(chat),
      onTap: () => _open(chat),
      onLongPress: () => _muteSheet(chat),
    );
    if (!(closed && chat.kind is ChatKindInfo_Dm)) return tile;
    // message-deletion: Remove a closed DM. The swipe never deletes by itself:
    // it asks first (a delete is never a bare swipe).
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
      // Always false: the row leaves through the reload that follows the core's
      // `ChatsChanged`, not through the Dismissible.
      confirmDismiss: (_) async {
        await _removeClosedDm(chat);
        return false;
      },
      child: tile,
    );
  }

  /// notification-prefs: Mute a conversation. The chat-row long-press menu:
  /// mute or unmute this conversation (offered for the lobby, a DM and a named
  /// group alike).
  Future<void> _muteSheet(ChatSummary chat) async {
    final muted = chat.muted;
    final action = await showModalBottomSheet<bool>(
      context: context,
      builder: (sheet) => SafeArea(
        child: ListView(
          shrinkWrap: true,
          children: [
            ListTile(
              key: ValueKey('chat-mute-toggle-${chat.chatId}'),
              leading: Icon(
                muted
                    ? Icons.notifications_active_outlined
                    : Icons.notifications_off_outlined,
              ),
              title: Text(muted ? 'Unmute notifications' : 'Mute notifications'),
              onTap: () => Navigator.of(sheet).pop(!muted),
            ),
          ],
        ),
      ),
    );
    if (action == null) return;
    try {
      await widget.core.setConversationMuted(chat.chatId, action);
      if (mounted) await _load();
    } catch (error) {
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(errorText(error))));
    }
  }

  Future<void> _removeClosedDm(ChatSummary chat) async {
    final peer = switch (chat.kind) {
      ChatKindInfo_Dm(:final peer) => _names.short(peer),
      _ => 'this chat',
    };
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialog) => AlertDialog(
        key: const Key('remove-closed-dm-dialog'),
        title: const Text('Remove this chat?'),
        content: Text(
          'The chat with $peer and its messages are deleted from this device '
          'and your other devices. This cannot be undone.',
        ),
        actions: [
          TextButton(
            key: const Key('remove-closed-dm-cancel'),
            onPressed: () => Navigator.of(dialog).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            key: const Key('remove-closed-dm-confirm'),
            onPressed: () => Navigator.of(dialog).pop(true),
            child: const Text('Remove'),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    try {
      await widget.core.removeClosedDm(chat.chatId);
      if (mounted) await _load();
    } catch (error) {
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(errorText(error))));
    }
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
          children: [
            // notification-prefs: Mute the whole app. While it is on, the list
            // conveys that notifications are globally off (not per row).
            if (_appMuted)
              MaterialBanner(
                key: const Key('app-muted-banner'),
                leading: const Icon(Icons.notifications_off_outlined),
                content: const Text(
                  'All notifications are muted. Turn it off in Profile.',
                ),
                actions: const [SizedBox.shrink()],
              ),
            for (final chat in _chats) _row(chat),
          ],
        ),
      ),
    ),
  );
}
