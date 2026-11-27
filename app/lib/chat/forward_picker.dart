// app-ui: Forward a message; Vyajena is a share target (change
// `message-actions-share`). A one-chat picker: the user's chats (the lobby,
// named groups and open DMs), minus the source chat and any closed chat. The
// Forward sheet and the inbound-share intake both use it to choose where content
// goes. Nothing is sent here: the caller sends into the chosen chat.
import 'package:flutter/material.dart';

import '../app/names.dart';
import '../core/core_service.dart';
import 'chat_model.dart';

/// The sheet's title.
const forwardToTitle = 'Forward to';

/// The sheet's title when choosing where shared content goes.
const shareToTitle = 'Send to';

/// A chat the user may forward or share into: its id and how to show it.
class ForwardTarget {
  const ForwardTarget(this.chatId, this.title);

  /// The chat's id (0 is the lobby).
  final int chatId;

  /// How the chat is shown in the picker.
  final String title;
}

/// Picks one chat to forward or share content into.
class ForwardPicker {
  /// Opens the picker over [context] and returns the chosen chat, or null if
  /// the user dismissed it. Lists the user's chats from the core, titled per
  /// kind (lobby / group name / DM peer), excluding [excludeChatId] (the source
  /// of a forward) and any closed group or DM, which are read-only. [title] is
  /// the sheet's heading ([forwardToTitle] for a forward, [shareToTitle] for an
  /// inbound share).
  static Future<ForwardTarget?> pick(
    BuildContext context,
    CoreService core, {
    int? excludeChatId,
    String title = forwardToTitle,
  }) async {
    final targets = await _targets(core, excludeChatId);
    if (!context.mounted) return null;
    if (targets.isEmpty) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(const SnackBar(content: Text('No other chat to send to')));
      return null;
    }
    return showModalBottomSheet<ForwardTarget>(
      context: context,
      builder: (context) => SafeArea(
        child: Column(
          key: const Key('forward-picker'),
          mainAxisSize: MainAxisSize.min,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 16, 16, 8),
              child: Align(
                alignment: Alignment.centerLeft,
                child: Text(title, style: Theme.of(context).textTheme.titleMedium),
              ),
            ),
            Flexible(
              child: ListView(
                shrinkWrap: true,
                children: [
                  for (final target in targets)
                    ListTile(
                      key: Key('forward-to-${target.chatId}'),
                      leading: const Icon(Icons.chat_bubble_outline),
                      title: Text(target.title),
                      onTap: () => Navigator.of(context).pop(target),
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  /// The chats the user may send into, titled, with [excludeChatId] and closed
  /// chats removed.
  static Future<List<ForwardTarget>> _targets(
    CoreService core,
    int? excludeChatId,
  ) async {
    final chats = await core.chats();
    final names = UserNames(core);
    await names.load([
      for (final c in chats)
        if (c.kind case ChatKindInfo_Dm(:final peer)) peer,
    ]);
    final out = <ForwardTarget>[];
    for (final chat in chats) {
      final id = chat.chatId.toInt();
      if (id == excludeChatId) continue;
      final String? title = switch (chat.kind) {
        ChatKindInfo_Lobby() => lobbyName,
        ChatKindInfo_Group(:final name, :final closed) =>
          closed ? null : (name ?? unnamedGroupName),
        ChatKindInfo_Dm(:final peer, :final closed) =>
          closed ? null : names.short(peer),
      };
      if (title != null) out.add(ForwardTarget(id, title));
    }
    return out;
  }
}
