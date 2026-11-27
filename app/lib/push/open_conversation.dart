// app-ui: A notification opens its conversation. Opens the chat a tapped
// notification names (by its conversation id), starting from the chat list, as a
// tap in the list would. An admin-event notification names no chat, or one that
// is gone: it opens the chat list.
import 'package:flutter/material.dart';

import '../chat/chat_model.dart';
import '../chat/chat_screen.dart';
import '../chat/dm_screen.dart';
import '../chat/group_screen.dart';
import '../core/core_service.dart';
import 'push_platform.dart';

/// Opens what [tap] names on [navigator].
Future<void> openTappedConversation({
  required NavigatorState navigator,
  required CoreService core,
  required ChatModel chat,
  required NotificationTap tap,
}) async {
  // The chat list is the first screen; a conversation sits on top of it.
  navigator.popUntil((route) => route.isFirst);
  final id = tap.chatId;
  if (id == null) return;
  final chats = await core.chats();
  final target = chats.where((c) => c.chatId == id).firstOrNull;
  if (target == null || !navigator.mounted) return;
  final admin = await core.isAdmin();
  if (!navigator.mounted) return;
  final context = navigator.context;
  switch (target.kind) {
    case ChatKindInfo_Lobby():
      await navigator.push(
        MaterialPageRoute<void>(
          builder: (_) => ChatScreen(core: core, chat: chat, admin: admin),
        ),
      );
    case ChatKindInfo_Group():
      // ignore: use_build_context_synchronously
      await showGroup(context, core, id, admin: admin);
    case ChatKindInfo_Dm(:final peer):
      // ignore: use_build_context_synchronously
      await showDm(context, core, id, peer);
  }
}
