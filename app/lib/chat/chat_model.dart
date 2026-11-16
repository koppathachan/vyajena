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

/// The newest refusal the core reported (a Welcome or a commit this device
/// could not use) and when it came: what the "waiting" screen shows while
/// the device is in no group (app-ui: Join from an invite).
class Refusal {
  const Refusal(this.reason, this.atMs);
  final String reason;

  /// When the core reported it, in ms since the Unix epoch.
  final int atMs;
}

/// The Pi's TLS key does not match the pin; the core stopped connecting.
class ServerIdentityNotice extends Notice {
  const ServerIdentityNotice();
}

/// This device was removed from the group (revoked); the core stopped
/// connecting (qa-fixes design D4).
class DeviceRemovedNotice extends Notice {
  const DeviceRemovedNotice();
}

/// [by] renamed the group to [name] (null: cleared the name).
class RenamedNotice extends Notice {
  const RenamedNotice(this.by, this.name);
  final UserRef by;
  final String? name;
}

/// What a membership notice says (qa-fixes design D5).
enum MemberChange {
  /// [MemberNotice.user]'s first device was added: "bob joined".
  joined,

  /// Another device of a member was added: "bob added a device".
  addedDevice,

  /// A user removed one of their own devices: "bob removed a device".
  removedDevice,

  /// Another user removed some of [MemberNotice.user]'s devices, not all:
  /// "akhil removed a device of bob".
  removedDeviceOf,

  /// All devices of a user were removed by another user: "akhil removed
  /// bob".
  removedUser,
}

/// A change to the group's members.
class MemberNotice extends Notice {
  const MemberNotice(this.change, this.user, this.by);
  final MemberChange change;
  final UserRef user;

  /// Who committed it.
  final UserRef by;
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

  /// The group's member devices as this model last knew them, by user:
  /// what a membership change is told apart by (first device, last
  /// device).
  final Map<String, Set<String>> _members = {};
  bool _removed = false;
  Refusal? _refusal;
  ReaddAnswer? _readd;
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

  /// The newest refusal since the device last joined; null: none.
  Refusal? get refusal => _refusal;

  /// The Pi's newest answer to this device asking to be added again, since
  /// it last joined (group-membership: Re-add after an unusable Welcome);
  /// null: it did not ask, or the answer changes nothing it waits for.
  ReaddAnswer? get readd => _readd;

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
      await _reloadMembers();
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

  /// Re-reads the member devices (none while not in the group).
  Future<void> _reloadMembers() async {
    List<MemberDevice> devices;
    try {
      devices = await core.members();
    } on Object {
      devices = const [];
    }
    _members.clear();
    for (final d in devices) {
      _members.putIfAbsent(_key(d.user), () => {}).add(d.device);
    }
  }

  /// The notices for one `MembersChanged`, from what the model knew before
  /// (qa-fixes design D5). Updates the known members.
  void _membersChanged(
    List<MemberDevice> added,
    List<MemberDevice> removed,
    UserRef? by,
    List<MemberDevice> automatic,
  ) {
    if (by == null || (added.isEmpty && removed.isEmpty)) {
      // This device joined (or rejoined), or created the group: no notice;
      // learn the members afresh.
      _reloadMembers();
      return;
    }
    final quiet = {for (final d in automatic) '${_key(d.user)}/${d.device}'};
    bool loud(MemberDevice d) => !quiet.contains('${_key(d.user)}/${d.device}');
    final users = <String, UserRef>{};
    final before = {
      for (final e in _members.entries) e.key: Set<String>.of(e.value),
    };
    for (final d in removed) {
      _members[_key(d.user)]?.remove(d.device);
    }
    for (final d in added) {
      _members.putIfAbsent(_key(d.user), () => {}).add(d.device);
    }
    _members.removeWhere((_, devices) => devices.isEmpty);

    final notices = <Notice>[];
    // Removals first, then additions (a commit that does both reads that
    // way), each user once, in the commit's order.
    for (final d in removed.where(loud)) {
      final key = _key(d.user);
      if (users.containsKey('-$key')) continue;
      users['-$key'] = d.user;
      final MemberChange change;
      if (_same(d.user, by)) {
        change = MemberChange.removedDevice;
      } else if (_members[key]?.isNotEmpty ?? false) {
        change = MemberChange.removedDeviceOf;
      } else {
        change = MemberChange.removedUser;
      }
      notices.add(MemberNotice(change, d.user, by));
    }
    for (final d in added.where(loud)) {
      final key = _key(d.user);
      if (users.containsKey('+$key')) continue;
      users['+$key'] = d.user;
      final had = before[key]?.difference({
        for (final r in removed)
          if (_key(r.user) == key) r.device,
      });
      notices.add(
        MemberNotice(
          had == null || had.isEmpty
              ? MemberChange.joined
              : MemberChange.addedDevice,
          d.user,
          by,
        ),
      );
    }
    if (notices.isEmpty) return;
    names
        .load([by, for (final n in notices) (n as MemberNotice).user])
        .then((_) => _changed());
    notices.forEach(_notice);
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
      case CoreEvent_GroupSettingsChanged(
        :final name,
        :final previous,
        :final by,
      ):
        _groupName = name;
        // `previous` is null on this device's first sync: no notice then.
        if (previous != null && previous != name) {
          names.load([by]).then((_) => _changed());
          _notice(RenamedNotice(by, name));
        } else {
          _changed();
        }
      case CoreEvent_Unreadable(:final sender, :final acceptedAtMs):
        if (sender != null) {
          names.load([sender]).then((_) => _changed());
        }
        _notice(UnreadableNotice(sender, acceptedAtMs));
      case CoreEvent_Gap(:final newestExpiredAtMs):
        _notice(GapNotice(newestExpiredAtMs));
      case CoreEvent_RemovedForInactivity(:final missedSinceMs):
        _notice(RemovedNotice(missedSinceMs));
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
        _refusal = Refusal(reason, DateTime.now().millisecondsSinceEpoch);
        _notice(CommitRefusedNotice(reason));
      case CoreEvent_ReaddRequested(:final outcome):
        if (outcome == ReaddAnswer.asked ||
            outcome == ReaddAnswer.limitReached) {
          _readd = outcome;
          _changed();
        }
      case CoreEvent_ServerIdentityError():
        _notice(const ServerIdentityNotice());
      case CoreEvent_MembersChanged(
        :final added,
        :final removed,
        :final by,
        :final automatic,
      ):
        _membersChanged(added, removed, by, automatic);
      case CoreEvent_Removed():
        // Emitted again on each start (an app restart): one notice.
        if (!_removed) {
          _removed = true;
          _notice(const DeviceRemovedNotice());
        }
      case CoreEvent_Rejoined():
        _refusal = null;
        _readd = null;
        _notice(const RejoinedNotice());
        _reloadMembers();
        _reload();
      case CoreEvent_JoinedGroup():
        _refusal = null;
        _readd = null;
        _reloadMembers();
      case CoreEvent_Connection() || CoreEvent_Other():
        break;
    }
  }

  static String _key(UserRef user) => '${user.username}@${user.serverId}';

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
