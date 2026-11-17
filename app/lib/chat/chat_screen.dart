// app-ui: Chat screen; Notices in the chat; Admin screens (the menu shows
// the admin entries only on admin devices).
//
// app-ui: Mentions of me; Reply to a message (change `mentions`, design D5,
// D6). The app bar's "@" badge counts unseen mentions; a mention is seen
// once its bubble was on screen while the app is in the foreground, or
// when the badge takes the user to it. Tapping a reply's quote scrolls to
// the original when it is among the loaded messages.
import 'package:flutter/material.dart';

import '../app/routes.dart';
import '../app/shell_app_bar.dart';
import '../core/core_service.dart';
import 'chat_model.dart';
import 'composer.dart';
import 'mentions.dart';
import 'message_tile.dart';
import 'notice_tile.dart';

/// The group's chat.
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

  @override
  void initState() {
    super.initState();
    _model.ensureLoaded();
  }

  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  String _name(Object user) => _model.names.of(user as UserRef);

  GlobalKey _tileKey(int id) => _tiles.putIfAbsent(id, GlobalKey.new);

  String _mentionLabel(MentionTarget target) => switch (target) {
    MentionTarget_User(:final user) => _model.names.short(user),
    MentionTarget_Everyone() => everyoneLabel,
  };

  /// The picker's choices: the other members, then everyone.
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
      const MentionChoice.everyone(),
    ];
  }

  void _startReply(ChatMessage message) {
    final body = message.body;
    setState(
      () => _reply = ReplyDraft(
        id: message.id,
        name: message.ownUser ? 'yourself' : _model.names.of(message.sender),
        quote: body is ChatBody_Text ? body.text : unsupportedText,
      ),
    );
  }

  Future<void> _send(String text, List<MentionSpan> mentions) async {
    await _model.send(text, mentions: mentions, replyTo: _reply?.id);
    if (mounted) setState(() => _reply = null);
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
      _queueCheck();
      return Scaffold(
        appBar: shellAppBar(
          widget.core,
          title: _model.groupName ?? 'Vyajena',
          actions: [
            _mentionBadge(),
            ChatMenu(admin: widget.admin),
          ],
        ),
        body: Column(
          children: [
            Expanded(
              child: items.isEmpty
                  ? const Center(child: Text('No messages yet'))
                  : NotificationListener<ScrollNotification>(
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
                            MessageItem(:final message) => KeyedSubtree(
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
                                onReply: widget.core.removed.value
                                    ? null
                                    : () => _startReply(message),
                                onQuoteTap: switch (message.reply) {
                                  final ReplyQuote r => () => _openOriginal(r),
                                  null => null,
                                },
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
            ),
            Composer(
              core: widget.core,
              send: _send,
              choices: _choices,
              reply: _reply,
              onCancelReply: () => setState(() => _reply = null),
            ),
          ],
        ),
      );
    },
  );
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
