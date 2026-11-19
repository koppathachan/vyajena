// app-ui: Chat screen; Notices in the chat; Admin screens (the menu shows
// the admin entries only on admin devices).
//
// app-ui: Mentions of me; Reply to a message (change `mentions`, design D5,
// D6). The app bar's "@" badge counts unseen mentions; a mention is seen
// once its bubble was on screen while the app is in the foreground, or
// when the badge takes the user to it. Tapping a reply's quote scrolls to
// the original when it is among the loaded messages.
import 'dart:typed_data';

import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../app/routes.dart';
import '../app/shell_app_bar.dart';
import '../core/core_service.dart';
import 'chat_model.dart';
import 'composer.dart';
import 'mentions.dart';
import 'message_tile.dart';
import 'notice_tile.dart';
import 'picture_picker.dart';

/// "Waiting for Bobby's devices": a DM before the peer's first device is
/// in it (app-ui: Open a DM).
String dmWaitingText(String name) =>
    "Waiting for $name's devices. Your messages go out once one is added.";

/// The banner of a closed DM (app-ui: DM screen).
String dmClosedText(String name) => '$name was removed';

/// A chat: the group's, or a DM's when [chat] is a DM's model (change
/// `dms`; app-ui: DM screen).
class ChatScreen extends StatefulWidget {
  const ChatScreen({
    super.key,
    required this.core,
    required this.chat,
    required this.admin,
  });

  final CoreService core;

  /// The app-level chat model (it outlives this screen).
  final ChatModel chat;

  /// Whether this device's user is the admin.
  final bool admin;

  @override
  State<ChatScreen> createState() => _ChatScreenState();
}

/// What the chat says when a reply's original is not on this device.
const originalMissingText = 'The original message is not on this device.';

class _ChatScreenState extends State<ChatScreen> {
  ChatModel get _model => widget.chat;
  final _scroll = ScrollController();
  final _list = GlobalKey();
  final Map<int, GlobalKey> _tiles = {};
  ReplyDraft? _reply;
  bool _checkQueued = false;

  /// While the chat scrolls on its own (to a quote's original or the
  /// badge's mention), what it passes on the way is not counted as seen.
  bool _jumping = false;

  int? _readUpTo;

  /// The read mark as it stood when this screen opened: the "New messages"
  /// divider pins to it, so it stays put while the stored mark advances
  /// (app-ui: Unread divider). Null until [_snapshotLoaded].
  int? _openMark;
  bool _snapshotLoaded = false;

  /// Whether the open-time scroll (to the first unread message, else the
  /// bottom) has run.
  bool _openPositioned = false;

  /// Whether the list is at (or near) the newest message: the
  /// scroll-to-bottom button shows only when it is not.
  bool _atBottom = true;

  /// How far from offset 0 (the newest message, the list is reversed) still
  /// counts as "at the bottom".
  static const _bottomThreshold = 80.0;

  @override
  void initState() {
    super.initState();
    _model.ensureLoaded();
    _loadSnapshot();
    _scroll.addListener(_onScroll);
  }

  /// Reads the read mark once, before [_markRead] advances it, so the
  /// divider pins to where the user was (app-ui: Unread divider).
  Future<void> _loadSnapshot() async {
    int mark;
    try {
      mark = await widget.core.chatReadMark(_model.chatId);
    } on Object {
      mark = 0;
    }
    if (!mounted) return;
    setState(() {
      _openMark = mark;
      _snapshotLoaded = true;
    });
  }

  void _onScroll() {
    if (!_scroll.hasClients) return;
    final atBottom = _scroll.offset <= _bottomThreshold;
    if (atBottom != _atBottom) setState(() => _atBottom = atBottom);
  }

  void _toBottom() => _scroll.animateTo(
    0,
    duration: const Duration(milliseconds: 250),
    curve: Curves.easeOut,
  );

  /// On open, lands on the first unread message (else stays at the bottom),
  /// once the read mark and the history are both loaded.
  void _positionOnOpen(int? firstUnreadId, List<ChatItem> items) {
    if (_openPositioned || !_snapshotLoaded) return;
    if (!items.any((item) => item is MessageItem)) return;
    _openPositioned = true;
    if (firstUnreadId != null) _jumpTo(firstUnreadId);
  }

  /// The chat list's unread mark follows what this screen showed while the
  /// app is in the foreground.
  void _markRead(List<ChatItem> items) {
    // Hold off until the divider has snapshotted the old mark (app-ui:
    // Unread divider), so reading it always wins the race with this write.
    if (!_snapshotLoaded) return;
    final last = items.whereType<MessageItem>().lastOrNull?.message.id;
    if (last == null || last == _readUpTo) return;
    final state = WidgetsBinding.instance.lifecycleState;
    if (state != null && state != AppLifecycleState.resumed) return;
    _readUpTo = last;
    _model.markRead();
  }

  @override
  void dispose() {
    _scroll.removeListener(_onScroll);
    _scroll.dispose();
    super.dispose();
  }

  String _name(Object user) => _model.names.of(user as UserRef);

  GlobalKey _tileKey(int id) => _tiles.putIfAbsent(id, GlobalKey.new);

  /// Draws the "New messages" divider above [tile] when [id] is the first
  /// unread message (app-ui: Unread divider).
  Widget _withDivider(int id, int? dividerBeforeId, Widget tile) =>
      id == dividerBeforeId
      ? Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [const _NewMessagesDivider(), tile],
        )
      : tile;

  String _mentionLabel(MentionTarget target) => switch (target) {
    MentionTarget_User(:final user) => _model.names.short(user),
    MentionTarget_Everyone() => everyoneLabel,
  };

  /// The picker's choices: the other members, then everyone (not in a DM).
  List<MentionChoice> _choices() {
    final me = widget.core.user.value;
    return [
      for (final user in _model.memberUsers)
        if (me == null ||
            user.username != me.username ||
            user.serverId != me.serverId)
          MentionChoice.user(
            user,
            _model.names.short(user),
            _model.names.of(user),
          ),
      if (_model.isMain) const MentionChoice.everyone(),
    ];
  }

  void _startReply(ChatMessage message) {
    final body = message.body;
    setState(
      () => _reply = ReplyDraft(
        id: message.id,
        name: message.ownUser ? 'yourself' : _model.names.of(message.sender),
        quote: switch (body) {
          ChatBody_Text(:final text) => text,
          ChatBody_Image(:final image) => image.caption ?? pictureQuote,
          _ => unsupportedText,
        },
      ),
    );
  }

  Future<void> _send(String text, List<MentionSpan> mentions) async {
    await _model.send(text, mentions: mentions, replyTo: _reply?.id);
    if (mounted) setState(() => _reply = null);
  }

  Future<void> _sendImage(
    Uint8List picked,
    String? caption,
    List<MentionSpan> mentions,
  ) => _model.sendImage(picked, caption: caption, mentions: mentions);

  Future<void> _resendImage(ChatMessage message) async {
    final messenger = ScaffoldMessenger.of(context);
    try {
      await _model.resendImage(message.id);
    } catch (error) {
      messenger.showSnackBar(SnackBar(content: Text(errorText(error))));
    }
  }

  /// Whether the tile of message [id] is at least partly inside the list's
  /// viewport.
  bool _onScreen(int id) {
    final tile = _tiles[id]?.currentContext?.findRenderObject();
    final list = _list.currentContext?.findRenderObject();
    if (tile is! RenderBox || list is! RenderBox) return false;
    if (!tile.attached || !list.attached || !tile.hasSize) return false;
    final top = tile.localToGlobal(Offset.zero);
    final rect = top & tile.size;
    final view = list.localToGlobal(Offset.zero) & list.size;
    return rect.overlaps(view);
  }

  /// Marks every unseen mention on screen seen, while the app is in the
  /// foreground (app-ui: Mentions of me).
  void _checkSeen() {
    _checkQueued = false;
    if (!mounted || _jumping) return;
    final state = WidgetsBinding.instance.lifecycleState;
    if (state != null && state != AppLifecycleState.resumed) return;
    for (final id in List.of(_model.unseenMentions)) {
      if (_onScreen(id)) _model.markSeen(id);
    }
  }

  void _queueCheck() {
    if (_checkQueued) return;
    _checkQueued = true;
    WidgetsBinding.instance.addPostFrameCallback((_) => _checkSeen());
  }

  /// Scrolls until message [id]'s tile is built, then brings it into view.
  /// False if it is not among the loaded messages.
  Future<bool> _jumpTo(int id) async {
    _jumping = true;
    try {
      return await _scrollTo(id);
    } finally {
      _jumping = false;
      _queueCheck();
    }
  }

  Future<bool> _scrollTo(int id) async {
    final loaded = _model.items.any(
      (item) => item is MessageItem && item.message.id == id,
    );
    if (!loaded || !_scroll.hasClients) return false;
    // Newest first is offset 0 (the list is reversed): walk towards older
    // messages a screen at a time until the tile exists.
    if (_tiles[id]?.currentContext == null) _scroll.jumpTo(0);
    for (var step = 0; step < 200; step++) {
      final context = _tiles[id]?.currentContext;
      if (context != null && context.mounted) {
        await Scrollable.ensureVisible(context, alignment: 0.5);
        return true;
      }
      final position = _scroll.position;
      if (position.pixels >= position.maxScrollExtent) return false;
      _scroll.jumpTo(
        (position.pixels + position.viewportDimension * 0.8).clamp(
          0,
          position.maxScrollExtent,
        ),
      );
      await WidgetsBinding.instance.endOfFrame;
      if (!mounted) return false;
    }
    return false;
  }

  Future<void> _openOriginal(ReplyQuote reply) async {
    final id = reply.originalId;
    final found = id != null && await _jumpTo(id);
    if (!found && mounted) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(const SnackBar(content: Text(originalMissingText)));
    }
  }

  /// The badge: jump to the oldest unseen mention and count it seen.
  Future<void> _nextMention() async {
    final unseen = _model.unseenMentions;
    if (unseen.isEmpty) return;
    final id = unseen.first;
    await _model.markSeen(id);
    await _jumpTo(id);
  }

  Widget _mentionBadge() {
    final count = _model.unseenMentions.length;
    if (count == 0) return const SizedBox.shrink();
    return IconButton(
      key: const Key('mention-badge'),
      tooltip: count == 1 ? '1 unseen mention' : '$count unseen mentions',
      onPressed: _nextMention,
      icon: Badge(
        label: Text('$count'),
        child: const Icon(Icons.alternate_email),
      ),
    );
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: _model,
    builder: (context, _) {
      final items = _model.items;
      final me = widget.core.user.value;
      final peer = _model.peer;
      final peerName = peer == null ? '' : _model.names.short(peer);
      // The unread divider pins to the read mark as it was on open: the
      // first received message past it is the first unread; the divider sits
      // above it unless it is the oldest message shown (app-ui: Unread
      // divider).
      final snapshot = _openMark;
      int? firstUnreadId;
      int? dividerBeforeId;
      if (snapshot != null) {
        var sawMessage = false;
        for (final item in items) {
          if (item is! MessageItem) continue;
          final message = item.message;
          if (!message.ownUser && message.id > snapshot) {
            firstUnreadId = message.id;
            if (sawMessage) dividerBeforeId = message.id;
            break;
          }
          sawMessage = true;
        }
      }
      _queueCheck();
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted) return;
        _positionOnOpen(firstUnreadId, items);
        _markRead(items);
      });
      return Scaffold(
        appBar: shellAppBar(
          widget.core,
          title: peer == null ? _model.groupName ?? 'Vyajena' : peerName,
          actions: [
            _mentionBadge(),
            // A DM shows no admin actions (app-ui: DM screen).
            ChatMenu(admin: widget.admin && _model.isMain),
          ],
        ),
        body: Column(
          children: [
            Expanded(
              child: Stack(
                children: [
                  if (items.isEmpty)
                    const Center(child: Text('No messages yet'))
                  else
                    NotificationListener<ScrollNotification>(
                      onNotification: (_) {
                        _queueCheck();
                        return false;
                      },
                      child: ListView.builder(
                        key: _list,
                        controller: _scroll,
                        reverse: true,
                        padding: const EdgeInsets.symmetric(vertical: 8),
                        itemCount: items.length,
                        itemBuilder: (context, index) {
                          final item = items[items.length - 1 - index];
                          return switch (item) {
                            MessageItem(:final message) => _withDivider(
                              message.id,
                              dividerBeforeId,
                              KeyedSubtree(
                                key: _tileKey(message.id),
                                child: MessageTile(
                                  key: ValueKey('message-${message.id}'),
                                  message: message,
                                  sender: _model.names.of(message.sender),
                                  mentionLabel: _mentionLabel,
                                  me: me,
                                  replyTo: switch (message.reply) {
                                    final ReplyQuote r => _model.names.of(r.to),
                                    null => null,
                                  },
                                  core: widget.core,
                                  onResendImage: () => _resendImage(message),
                                  onReply: widget.core.removed.value
                                      ? null
                                      : () => _startReply(message),
                                  onQuoteTap: switch (message.reply) {
                                    final ReplyQuote r => () => _openOriginal(
                                      r,
                                    ),
                                    null => null,
                                  },
                                ),
                              ),
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
                  if (items.isNotEmpty && !_atBottom)
                    Positioned(
                      right: 16,
                      bottom: 16,
                      child: FloatingActionButton.small(
                        key: const Key('scroll-to-bottom'),
                        tooltip: 'Scroll to latest',
                        onPressed: _toBottom,
                        child: const Icon(Icons.arrow_downward),
                      ),
                    ),
                ],
              ),
            ),
            if (_model.closed)
              MaterialBanner(
                key: const Key('dm-closed'),
                content: Text(dmClosedText(peerName)),
                actions: const [SizedBox.shrink()],
              )
            else ...[
              if (_model.waiting)
                Padding(
                  key: const Key('dm-waiting'),
                  padding: const EdgeInsets.symmetric(
                    horizontal: 16,
                    vertical: 4,
                  ),
                  child: Text(
                    dmWaitingText(peerName),
                    style: Theme.of(context).textTheme.bodySmall,
                  ),
                ),
              Composer(
                core: widget.core,
                send: _send,
                pickImage: PicturePickerScope.of(context).pick,
                sendImage: _sendImage,
                choices: _choices,
                reply: _reply,
                onCancelReply: () => setState(() => _reply = null),
              ),
            ],
          ],
        ),
      );
    },
  );
}

/// The "New messages" line above the first unread message (app-ui: Unread
/// divider).
class _NewMessagesDivider extends StatelessWidget {
  const _NewMessagesDivider();

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Padding(
      key: const Key('unread-divider'),
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      child: Row(
        children: [
          Expanded(child: Divider(color: scheme.primary)),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 8),
            child: Text(
              'New messages',
              style: Theme.of(context).textTheme.labelSmall?.copyWith(
                color: scheme.primary,
                fontWeight: FontWeight.w600,
              ),
            ),
          ),
          Expanded(child: Divider(color: scheme.primary)),
        ],
      ),
    );
  }
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
