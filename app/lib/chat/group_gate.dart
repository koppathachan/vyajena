// What a device with a user sees: on the admin's first start, creating the
// group (app-ui: Admin screens); on a new member's device, that it waits
// for the admin's device to add it (app-ui: Join from an invite, "Waiting to
// be added"); and the chat once the device is in the group.
import 'dart:async';

import 'package:flutter/material.dart';

import '../admin/create_group_screen.dart';
import '../app/shell_app_bar.dart';
import '../core/core_service.dart';
import 'chat_screen.dart';

/// What this device waits for before the chat.
const waitingText =
    "Waiting for the admin's device to add you to the group. This happens "
    'on its own as soon as it is online.';

/// Create the group, wait to be added, or chat.
class GroupGate extends StatefulWidget {
  const GroupGate({super.key, required this.core});

  final CoreService core;

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
    if (inGroup) return ChatScreen(core: widget.core, admin: admin);
    if (admin) return CreateGroupScreen(core: widget.core, onCreated: _check);
    return Scaffold(
      appBar: shellAppBar(widget.core),
      body: const Center(
        child: Padding(
          padding: EdgeInsets.all(24),
          child: Text(waitingText, textAlign: TextAlign.center),
        ),
      ),
    );
  }
}
