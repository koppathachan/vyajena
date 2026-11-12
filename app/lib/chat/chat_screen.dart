// app-ui: Chat screen; Notices in the chat; Admin screens (the menu shows
// the admin entries only on admin devices).
import 'package:flutter/material.dart';

import '../app/routes.dart';
import '../app/shell_app_bar.dart';
import '../core/core_service.dart';
import 'chat_model.dart';
import 'composer.dart';
import 'message_tile.dart';
import 'notice_tile.dart';

/// The group's chat.
class ChatScreen extends StatefulWidget {
  const ChatScreen({super.key, required this.core, required this.admin});

  final CoreService core;

  /// Whether this device's user is the admin.
  final bool admin;

  @override
  State<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends State<ChatScreen> {
  late final ChatModel _model = ChatModel(widget.core);

  @override
  void initState() {
    super.initState();
    _model.start();
  }

  @override
  void dispose() {
    _model.dispose();
    super.dispose();
  }

  String _name(Object user) => _model.names.of(user as UserRef);

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: _model,
    builder: (context, _) {
      final items = _model.items;
      return Scaffold(
        appBar: shellAppBar(
          widget.core,
          title: _model.groupName ?? 'Vyajena',
          actions: [ChatMenu(admin: widget.admin)],
        ),
        body: Column(
          children: [
            Expanded(
              child: items.isEmpty
                  ? const Center(child: Text('No messages yet'))
                  : ListView.builder(
                      reverse: true,
                      padding: const EdgeInsets.symmetric(vertical: 8),
                      itemCount: items.length,
                      itemBuilder: (context, index) {
                        final item = items[items.length - 1 - index];
                        return switch (item) {
                          MessageItem(:final message) => MessageTile(
                            key: ValueKey('message-${message.id}'),
                            message: message,
                            sender: _model.names.of(message.sender),
                          ),
                          NoticeItem(:final notice) => NoticeTile(
                            notice: notice,
                            text: noticeText(context, notice, _name),
                            onAccept:
                                widget.admin &&
                                    notice is KeyChangedNotice &&
                                    !notice.accepted
                                ? () => _model.accept(notice)
                                : null,
                          ),
                        };
                      },
                    ),
            ),
            Composer(core: widget.core, send: _model.send),
          ],
        ),
      );
    },
  );
}

/// The chat's menu entries.
enum MenuEntry {
  members('Members', Routes.members),
  profile('Profile', Routes.profile),
  linkDevice('Link a new device', Routes.approveLink),
  invite('Invite a user', Routes.invite),
  invites('Invites', Routes.invites),
  removeUser('Remove a user', Routes.removeUser),
  groupName('Group name', Routes.groupName);

  const MenuEntry(this.label, this.route);

  final String label;
  final String route;

  /// Whether only admin devices show it.
  bool get adminOnly => switch (this) {
    invite || invites || removeUser || groupName => true,
    members || profile || linkDevice => false,
  };
}

/// The chat's menu: everyone's entries, and the admin's only on admin
/// devices (the Pi rejects other devices' admin calls anyway).
class ChatMenu extends StatelessWidget {
  const ChatMenu({super.key, required this.admin});

  final bool admin;

  @override
  Widget build(BuildContext context) => PopupMenuButton<MenuEntry>(
    tooltip: 'Menu',
    onSelected: (entry) => Navigator.of(context).pushNamed(entry.route),
    itemBuilder: (context) => [
      for (final entry in MenuEntry.values)
        if (admin || !entry.adminOnly)
          PopupMenuItem(value: entry, child: Text(entry.label)),
    ],
  );
}
