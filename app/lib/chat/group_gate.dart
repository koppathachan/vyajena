// What a device with a user sees: on the admin's first start, creating the
// group (app-ui: Admin screens); on a new member's device, that it waits
// for the admin's device to add it (app-ui: Join from an invite, "Waiting to
// be added"); and the chat once the device is in the group.
import 'dart:async';

import 'package:flutter/material.dart';

import '../admin/create_group_screen.dart';
import '../app/shell_app_bar.dart';
import '../core/core_service.dart';
import 'chat_model.dart';
import 'chat_screen.dart';
import 'time_text.dart';

/// What this device waits for before the chat.
const waitingText =
    "Waiting for the admin's device to add you to the group. This happens "
    'on its own as soon as it is online.';

/// What the waiting screen says once the core refused what the admin's
/// device sent to add it (a Welcome it could not use). Owner's text
/// (welcome-recovery design D6), with the reason and when below it.
const refusedText =
    "Your phone couldn't open the admin's invitation to the group.";

/// ... once the core asked the Pi to be added again (group-membership:
/// Re-add after an unusable Welcome).
const readdAskedText =
    'It has asked to be added again; this happens on its own.';

/// ... once it asked too often today (the Pi's daily limit).
const readdLimitText =
    'It has asked to be added again too often today and now waits for the '
    'admin.';

/// `Reason: <error> (<time>)`.
String reasonText(BuildContext context, Refusal refusal) =>
    'Reason: ${refusal.reason} (${timeText(context, refusal.atMs)})';

/// Create the group, wait to be added, or chat.
class GroupGate extends StatefulWidget {
  const GroupGate({super.key, required this.core, required this.chat});

  final CoreService core;

  /// The app-level chat model.
  final ChatModel chat;

  @override
  State<GroupGate> createState() => _GroupGateState();
}

class _GroupGateState extends State<GroupGate> {
  bool? _admin;
  bool? _inGroup;
  StreamSubscription<CoreEvent>? _subscription;

  @override
  void initState() {
    super.initState();
    _subscription = widget.core.events.listen((event) {
      if (event is CoreEvent_JoinedGroup ||
          event is CoreEvent_MembersChanged ||
          event is CoreEvent_Rejoined) {
        _check();
      }
    });
    _check();
  }

  @override
  void dispose() {
    _subscription?.cancel();
    super.dispose();
  }

  Future<void> _check() async {
    final admin = await widget.core.isAdmin();
    final inGroup = await widget.core.epoch() != null;
    if (!mounted) return;
    setState(() {
      _admin = admin;
      _inGroup = inGroup;
    });
  }

  @override
  Widget build(BuildContext context) {
    final admin = _admin;
    final inGroup = _inGroup;
    if (admin == null || inGroup == null) {
      return Scaffold(appBar: shellAppBar(widget.core));
    }
    if (inGroup) {
      return ChatScreen(core: widget.core, chat: widget.chat, admin: admin);
    }
    if (admin) return CreateGroupScreen(core: widget.core, onCreated: _check);
    return Scaffold(
      appBar: shellAppBar(widget.core),
      body: Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: ListenableBuilder(
            listenable: widget.chat,
            builder: (context, _) =>
                _waiting(context, widget.chat.refusal, widget.chat.readd),
          ),
        ),
      ),
    );
  }

  Widget _waiting(BuildContext context, Refusal? refusal, ReaddAnswer? readd) {
    const waiting = Text(waitingText, textAlign: TextAlign.center);
    if (refusal == null && readd == null) return waiting;
    final theme = Theme.of(context);
    final asked = switch (readd) {
      ReaddAnswer.limitReached => readdLimitText,
      ReaddAnswer.asked => readdAskedText,
      _ => null,
    };
    return Card.outlined(
      key: const Key('refusal'),
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              refusedText,
              textAlign: TextAlign.center,
              style: theme.textTheme.titleSmall,
            ),
            if (asked != null) ...[
              const SizedBox(height: 8),
              Text(asked, key: const Key('readd'), textAlign: TextAlign.center),
            ],
            if (refusal != null) ...[
              const SizedBox(height: 8),
              SelectableText(
                reasonText(context, refusal),
                textAlign: TextAlign.center,
                style: theme.textTheme.bodySmall,
              ),
            ],
          ],
        ),
      ),
    );
  }
}
