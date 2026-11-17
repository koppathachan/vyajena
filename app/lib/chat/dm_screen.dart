// app-ui: Open a DM; DM screen (change `dms`). A DM is the chat screen over
// its own chat model, made when the screen opens and dropped when it
// closes: the core keeps the only copy of its messages.
import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../core/core_service.dart';
import 'chat_model.dart';
import 'chat_screen.dart';

/// The DM [chatId] with [peer].
class DmScreen extends StatefulWidget {
  const DmScreen({
    super.key,
    required this.core,
    required this.chatId,
    required this.peer,
  });

  final CoreService core;
  final int chatId;
  final UserRef peer;

  @override
  State<DmScreen> createState() => _DmScreenState();
}

class _DmScreenState extends State<DmScreen> {
  late final ChatModel _model = ChatModel(
    widget.core,
    chatId: widget.chatId,
    peer: widget.peer,
  );

  @override
  void dispose() {
    _model.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) =>
      ChatScreen(core: widget.core, chat: _model, admin: false);
}

/// Opens the DM [chatId] with [peer].
Future<void> showDm(
  BuildContext context,
  CoreService core,
  int chatId,
  UserRef peer,
) => Navigator.of(context).push(
  MaterialPageRoute<void>(
    builder: (_) => DmScreen(core: core, chatId: chatId, peer: peer),
  ),
);

/// "Message" on [user] (app-ui: Open a DM): the open DM with them, or a new
/// one the core opens first; shown at once.
Future<void> messageUser(
  BuildContext context,
  CoreService core,
  UserRef user,
) async {
  final messenger = ScaffoldMessenger.of(context);
  final navigator = Navigator.of(context);
  try {
    final chatId = await core.openDm(user);
    await navigator.push(
      MaterialPageRoute<void>(
        builder: (_) => DmScreen(core: core, chatId: chatId, peer: user),
      ),
    );
  } catch (error) {
    messenger.showSnackBar(SnackBar(content: Text(errorText(error))));
  }
}
