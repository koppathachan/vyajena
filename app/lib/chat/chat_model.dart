// app-ui: Chat screen; Notices in the chat. The chat's contents: the core's
// messages in server_seq order (always re-read from the core's history, so
// an own message accepted after a received one still lands in its place),
// with the notices the core emitted, each kept after the message that was
// last when it came. Nothing here is written anywhere: the core keeps the
// only copy of the messages.
//
// One ChatModel lives as long as the app (VyajenaApp makes it before the
// core opens), not the chat screen: notices the core emits while the chat
// is not on screen (on the "waiting" screen, or before the chat's first
// frame) are still shown, in order, once it is. It reads the history before
// the engine first starts, so a notice is placed after the message that was
// really last when it came.
import 'dart:async';

import 'package:flutter/foundation.dart';

import '../app/names.dart';
import '../core/core_service.dart';

/// How many messages the chat shows.
const historyLimit = 500;

/// Something the chat shows between messages.
sealed class Notice {
  const Notice();
}

/// Messages up to [newestExpiredAtMs] expired on the Pi before this device
/// fetched them.
class GapNotice extends Notice {
  const GapNotice(this.newestExpiredAtMs);
  final int newestExpiredAtMs;
}

/// This device was removed for inactivity; messages since [missedSinceMs]
/// were not kept for it.
class RemovedNotice extends Notice {
  const RemovedNotice(this.missedSinceMs);
  final int missedSinceMs;
}

/// This device is back in the group.
class RejoinedNotice extends Notice {
  const RejoinedNotice();
}

/// [user]'s identity key changed. [accepted] once the admin accepted it.
class KeyChangedNotice extends Notice {
  KeyChangedNotice(this.user);
  final UserRef user;
  bool accepted = false;
}

/// A message from [sender] could not be read.
class UnreadableNotice extends Notice {
  const UnreadableNotice(this.sender, this.acceptedAtMs);
  final UserRef? sender;
  final int? acceptedAtMs;
}

/// This device refused a commit and cannot follow the group until re-added.
class CommitRefusedNotice extends Notice {
  const CommitRefusedNotice(this.reason);
  final String reason;
}

/// The Pi's TLS key does not match the pin; the core stopped connecting.
class ServerIdentityNotice extends Notice {
  const ServerIdentityNotice();
}

/// One row of the chat.
sealed class ChatItem {
  const ChatItem();
}

/// A message.
class MessageItem extends ChatItem {
  const MessageItem(this.message);
  final ChatMessage message;
}

/// A notice.
class NoticeItem extends ChatItem {
  const NoticeItem(this.notice);
  final Notice notice;
}

class _Anchored {
  _Anchored(this.notice, this.afterId);
  final Notice notice;

  /// The message the notice follows; null: before every message.
  final int? afterId;
}

/// The chat's messages and notices, kept up to date from the core.
class ChatModel extends ChangeNotifier {
  /// Follows [core]'s events from now on; make it before the core opens.
  ChatModel(this.core) : names = UserNames(core) {
    _subscription = core.events.listen(_onEvent);
    core.addBeforeStart(_prime);
  }

  final CoreService core;
  final UserNames names;

  List<ChatMessage> _messages = const [];
  final List<_Anchored> _notices = [];
  String? _groupName;
  late final StreamSubscription<CoreEvent> _subscription;

  /// The newest message as events and sends came, which a new notice
  /// follows; null: none yet.
  int? _lastId;
  bool _primed = false;
  bool _loading = false;
  bool _again = false;
  bool _disposed = false;

  /// The group's name, once an admin device set it.
  String? get groupName => _groupName;

  /// The chat, oldest first.
  List<ChatItem> get items {
    final out = <ChatItem>[];
    final ids = {for (final m in _messages) m.id};
    void noticesAfter(int? id) {
      for (final n in _notices) {
        if (n.afterId == id) out.add(NoticeItem(n.notice));
      }
    }

    noticesAfter(null);
    for (final message in _messages) {
      out.add(MessageItem(message));
      noticesAfter(message.id);
    }
    // Notices after a message no longer in the window go last.
    for (final n in _notices) {
      if (n.afterId != null && !ids.contains(n.afterId)) {
        out.add(NoticeItem(n.notice));
      }
    }
    return out;
  }

  /// Reads the group name and the history once, before the engine first
  /// starts (or when the chat opens, if that comes first).
  Future<void> _prime() async {
    if (_primed) return;
    _primed = true;
    try {
      _groupName = await core.groupName();
      await _reload();
      _lastId ??= _messages.isEmpty ? null : _messages.last.id;
    } on Object {
      // Not readable yet (the core failed to open): try again next time.
      _primed = false;
    }
  }

  /// Makes sure the history is loaded; the chat screen calls it on open.
  Future<void> ensureLoaded() => _prime();

  @override
  void dispose() {
    _disposed = true;
    core.removeBeforeStart(_prime);
    _subscription.cancel();
    super.dispose();
  }

  void _changed() {
    if (!_disposed) notifyListeners();
  }

  /// Re-reads the history from the core (and the senders' names).
  Future<void> _reload() async {
    if (_loading) {
      _again = true;
      return;
    }
    _loading = true;
    try {
      do {
        _again = false;
        final messages = await core.history(historyLimit);
        await names.load(
          {
            for (final m in messages)
              '${m.sender.username}@${m.sender.serverId}': m.sender,
          }.values,
        );
        _messages = messages;
        _changed();
      } while (_again && !_disposed);
    } finally {
      _loading = false;
    }
  }

  void _notice(Notice notice) {
    _notices.add(_Anchored(notice, _lastId));
    _changed();
  }

  void _onEvent(CoreEvent event) {
    switch (event) {
      case CoreEvent_Message(:final message):
        _lastId = message.id;
        _reload();
      case CoreEvent_SendStatus():
        _reload();
      case CoreEvent_ProfileChanged(:final user):
        names.forget(user);
        names.load([user]).then((_) => _changed());
      case CoreEvent_GroupSettingsChanged(:final name):
        _groupName = name;
        _changed();
      case CoreEvent_Unreadable(:final sender, :final acceptedAtMs):
        if (sender != null) {
          names.load([sender]).then((_) => _changed());
        }
        _notice(UnreadableNotice(sender, acceptedAtMs));
      case CoreEvent_Gap(:final newestExpiredAtMs):
        _notice(GapNotice(newestExpiredAtMs));
      case CoreEvent_RemovedForInactivity(:final missedSinceMs):
        _notice(RemovedNotice(missedSinceMs));
      case CoreEvent_Rejoined():
        _notice(const RejoinedNotice());
        _reload();
      case CoreEvent_IdentityKeyChanged(:final user):
        // One warning per changed key until it is accepted (the core warns
        // again each time it meets the key).
        final open = _notices.any(
          (n) =>
              n.notice is KeyChangedNotice &&
              !(n.notice as KeyChangedNotice).accepted &&
              _same((n.notice as KeyChangedNotice).user, user),
        );
        if (!open) {
          names.load([user]).then((_) => _changed());
          _notice(KeyChangedNotice(user));
        }
      case CoreEvent_CommitRefused(:final reason):
        _notice(CommitRefusedNotice(reason));
      case CoreEvent_ServerIdentityError():
        _notice(const ServerIdentityNotice());
      case CoreEvent_JoinedGroup() ||
          CoreEvent_MembersChanged() ||
          CoreEvent_Removed() ||
          CoreEvent_Connection() ||
          CoreEvent_Other():
        break;
    }
  }

  static bool _same(UserRef a, UserRef b) =>
      a.username == b.username && a.serverId == b.serverId;

  /// Sends [text]; returns once the core stored it (it shows ⏳ until the
  /// Pi accepts it).
  Future<void> send(String text) async {
    _lastId = await core.sendText(text);
    await _reload();
  }

  /// Admin: accepts the changed key of [notice]'s user (the core then adds
  /// their waiting devices, group-membership: Automatic add of new users).
  Future<bool> accept(KeyChangedNotice notice) async {
    final accepted = await core.acceptIdentityKey(notice.user);
    notice.accepted = true;
    _changed();
    return accepted;
  }
}
