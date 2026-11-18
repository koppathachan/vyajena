// app-ui: Chat list (change `dms`). The group first, then the DMs by the
// peer's name, newest message first, closed ones last and greyed, each with
// an unread mark and the count of unseen mentions of this user.
import 'dart:async';

import 'package:flutter/material.dart';

import '../app/names.dart';
import '../app/shell_app_bar.dart';
import '../core/core_service.dart';
import 'chat_model.dart';
import 'chat_screen.dart';
import 'dm_screen.dart';

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
            CoreEvent_SendStatus() ||
            CoreEvent_GroupSettingsChanged():
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
    await _names.load([
      for (final c in chats)
        if (c.kind case ChatKindInfo_Dm(:final peer)) peer,
    ]);
    if (mounted) setState(() => _chats = chats);
  }

  Future<void> _open(ChatSummary chat) async {
    switch (chat.kind) {
      case ChatKindInfo_Main():
        await Navigator.of(context).push(
          MaterialPageRoute<void>(
            builder: (_) => ChatScreen(
              core: widget.core,
              chat: widget.chat,
              admin: widget.admin,
            ),
          ),
        );
      case ChatKindInfo_Dm(:final peer):
        await showDm(context, widget.core, chat.chatId, peer);
    }
    await _load();
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
      body: ListView(
        children: [
          for (final chat in _chats)
            switch (chat.kind) {
              ChatKindInfo_Main() => ListTile(
                key: const ValueKey('chat-main'),
                leading: const Icon(Icons.groups_outlined),
                title: Text(widget.chat.groupName ?? 'Group'),
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
  );
}
