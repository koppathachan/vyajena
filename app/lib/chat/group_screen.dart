// app-ui: Named group screen (change `lobby-multi-group`). A named group is
// the chat screen over its own chat model, made when the screen opens and
// dropped when it closes: the core keeps the only copy of its messages.
import 'package:flutter/material.dart';

import '../core/core_service.dart';
import 'chat_model.dart';
import 'chat_screen.dart';

/// The named group [chatId].
class GroupScreen extends StatefulWidget {
  const GroupScreen({
    super.key,
    required this.core,
    required this.chatId,
    required this.admin,
  });

  final CoreService core;
  final int chatId;

  /// Whether this device's user is the admin.
  final bool admin;

  @override
  State<GroupScreen> createState() => _GroupScreenState();
}

class _GroupScreenState extends State<GroupScreen> {
  late final ChatModel _model = ChatModel(
    widget.core,
    chatId: widget.chatId,
    group: true,
  );

  @override
  void dispose() {
    _model.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) =>
      ChatScreen(core: widget.core, chat: _model, admin: widget.admin);
}

/// Opens the named group [chatId].
Future<void> showGroup(
  BuildContext context,
  CoreService core,
  int chatId, {
  required bool admin,
}) => Navigator.of(context).push(
  MaterialPageRoute<void>(
    builder: (_) => GroupScreen(core: core, chatId: chatId, admin: admin),
  ),
);
