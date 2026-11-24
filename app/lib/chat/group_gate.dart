// What a device with a user sees: on the admin's first start, creating the
// group (app-ui: Admin screens); on a new member's device, that it waits
// for the admin's device to add it (app-ui: Join from an invite, "Waiting to
// be added"); and the chat once the device is in the group.
import 'dart:async';

import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../app/shell_app_bar.dart';
import '../core/core_service.dart';
import '../shell/home_shell.dart';
import 'chat_model.dart';
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

/// ... once the core asked the server to be added again (group-membership:
/// Re-add after an unusable Welcome).
const readdAskedText =
    'It has asked to be added again; this happens on its own.';

/// ... once it asked too often today (the server's daily limit).
const readdLimitText =
    'It has asked to be added again too often today and now waits for the '
    'admin.';

/// `Reason: <error> (<time>)`.
String reasonText(BuildContext context, Refusal refusal) =>
    'Reason: ${refusal.reason} (${timeText(context, refusal.atMs)})';

/// What the admin's first device shows while the lobby is being set up
/// automatically (app-ui: The lobby appears without a manual create step).
const settingUpText = 'Setting up the lobby…';

/// What the admin sees if the automatic lobby creation failed for a real
/// reason (not a benign "already exists" race): the reason, and a retry.
const createFailedText = "The lobby couldn't be created.";

/// Where the admin's first device is in setting up the lobby.
enum _LobbySetup {
  /// Not started: the device is the admin's and has no lobby yet.
  none,

  /// `Admin.CreateGroup` is in flight.
  creating,

  /// The server already has a lobby (a linked admin device): wait to be synced.
  waiting,

  /// Creation failed for a real reason: show the error and a retry.
  failed,
}

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
  _LobbySetup _setup = _LobbySetup.none;
  String? _createError;
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
    // The admin's first device creates the lobby on its own, with no manual
    // "Create the group" step (app-ui: The lobby appears without a manual
    // create step). Fire once, unless a real failure is showing its retry.
    if (admin && !inGroup && _setup == _LobbySetup.none) {
      _autoCreateLobby();
    }
  }

  /// Creates the lobby automatically on the admin's first device. A benign
  /// "already in a group" answer (a linked admin device racing a lobby that
  /// already exists) leaves the device waiting to be synced in; any other
  /// error shows a retry.
  Future<void> _autoCreateLobby() async {
    setState(() {
      _setup = _LobbySetup.creating;
      _createError = null;
    });
    try {
      await widget.core.createGroup();
      // The lobby exists now; re-check flips to the chat.
      if (mounted) await _check();
    } on CoreError catch (error) {
      if (!mounted) return;
      if (error.kind == CoreErrorKind.alreadyInGroup) {
        // Benign: the server already has the lobby. Wait to be synced in; the
        // event listener re-checks on JoinedGroup/Rejoined/MembersChanged.
        setState(() => _setup = _LobbySetup.waiting);
        return;
      }
      setState(() {
        _setup = _LobbySetup.failed;
        _createError = errorText(error);
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _setup = _LobbySetup.failed;
        _createError = errorText(error);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final admin = _admin;
    final inGroup = _inGroup;
    if (admin == null || inGroup == null) {
      return Scaffold(appBar: shellAppBar(widget.core));
    }
    // The app shell (Chats, Contacts, Profile) is always the home once this device is in the group
    // (app-ui: Chat list); the group's chat opens on top of it.
    if (inGroup) {
      return HomeShell(core: widget.core, chat: widget.chat, admin: admin);
    }
    if (admin && _setup == _LobbySetup.failed) {
      return Scaffold(
        appBar: shellAppBar(widget.core),
        body: Center(
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Column(
              key: const Key('lobby-create-failed'),
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  createFailedText,
                  textAlign: TextAlign.center,
                  style: Theme.of(context).textTheme.titleMedium,
                ),
                if (_createError case final error?) ...[
                  const SizedBox(height: 12),
                  Text(
                    error,
                    textAlign: TextAlign.center,
                    style: TextStyle(color: Theme.of(context).colorScheme.error),
                  ),
                ],
                const SizedBox(height: 16),
                FilledButton(
                  onPressed: _autoCreateLobby,
                  child: const Text('Try again'),
                ),
              ],
            ),
          ),
        ),
      );
    }
    // The admin's first device while the lobby is being created automatically.
    if (admin && _setup != _LobbySetup.waiting) {
      return Scaffold(
        appBar: shellAppBar(widget.core),
        body: const Center(
          child: Column(
            key: Key('lobby-setting-up'),
            mainAxisSize: MainAxisSize.min,
            children: [
              CircularProgressIndicator(),
              SizedBox(height: 16),
              Text(settingUpText, textAlign: TextAlign.center),
            ],
          ),
        ),
      );
    }
    // A member, or the admin's linked device waiting to be synced into the
    // existing lobby, waits to be added.
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
