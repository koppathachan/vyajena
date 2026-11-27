// app-ui: Chat screen; Notices in the chat; Admin screens (the menu shows
// the admin entries only on admin devices).
//
// app-ui: Mentions of me; Reply to a message (change `mentions`, design D5,
// D6). The app bar's "@" badge counts unseen mentions; a mention is seen
// once its bubble was on screen while the app is in the foreground, or
// when the badge takes the user to it. Tapping a reply's quote scrolls to
// the original when it is among the loaded messages.
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../app/arcade/sketch.dart';
import '../app/errors.dart';
import '../app/moderation.dart';
import '../app/routes.dart';
import '../app/connection_indicator.dart';
import '../shell/avatars.dart';
import '../core/core_service.dart';
import 'chat_model.dart';
import 'composer.dart';
import 'forward_picker.dart';
import 'group_menu.dart';
import 'mentions.dart';
import 'message_tile.dart';
import 'notice_tile.dart';
import 'picture_picker.dart';
import 'report_tile.dart';
import 'sticker_plugins.dart';
import 'video_plugins.dart';
import 'voice_playback.dart';
import 'voice_plugins.dart';

/// The banner of a closed named group (app-ui: Named group screen).
const groupClosedText = 'This group was closed';

/// The in-chat banner when a chat is out of sync: this device fell behind the
/// group's epoch and could not re-sync on its own (app-ui: Out-of-sync re-sync
/// affordance). The held message is kept, not lost.
const outOfSyncText = 'This chat is out of sync';

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
  /// Deleted DM messages the user revealed, shown in place (message-deletion:
  /// Receiver may reveal in a DM).
  final Map<int, ChatMessage> _revealed = {};
  ReplyDraft? _reply;

  /// The chat's one voice player: one clip plays at a time, and it stops when
  /// the chat closes (app-ui: Play a voice message).
  VoicePlayback? _playback;
  bool _checkQueued = false;

  /// While the chat scrolls on its own (to a quote's original or the
  /// badge's mention), what it passes on the way is not counted as seen.
  bool _jumping = false;

  int? _readUpTo;

  /// Whether this conversation is muted individually (notification-prefs: Mute
  /// a conversation), for the overflow menu's label.
  bool _muted = false;
  StreamSubscription<CoreEvent>? _muteSub;

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
    _loadMuted();
    _muteSub = widget.core.events.listen((event) {
      if (event is CoreEvent_MutePrefsChanged) _loadMuted();
    });
    _scroll.addListener(_onScroll);
  }

  /// Reads whether this conversation is muted, for the menu's label
  /// (notification-prefs: Mute a conversation).
  Future<void> _loadMuted() async {
    try {
      final chats = await widget.core.chats();
      bool? muted;
      for (final c in chats) {
        if (c.chatId == _model.chatId) {
          muted = c.muted;
          break;
        }
      }
      if (mounted && muted != null && muted != _muted) {
        setState(() => _muted = muted!);
      }
    } on Object {
      // The label falls back to "Mute"; not critical.
    }
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
  void didChangeDependencies() {
    super.didChangeDependencies();
    _playback ??= VoicePlayback(VoiceScope.of(context).newPlayer);
  }

  @override
  void dispose() {
    _muteSub?.cancel();
    _scroll.removeListener(_onScroll);
    _scroll.dispose();
    _playback?.dispose();
    super.dispose();
  }

  /// notification-prefs: Mute a conversation. Toggles this conversation's mute
  /// from the overflow menu.
  Future<void> _toggleMuted() async {
    try {
      await widget.core.setConversationMuted(_model.chatId, !_muted);
    } catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text(errorText(error))));
      }
    }
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
      // `@everyone` means everyone in this chat: the lobby or a named group
      // (a DM has just the two).
      if (_model.isMain || _model.isGroup) const MentionChoice.everyone(),
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
          ChatBody_Audio() => voiceQuote,
          ChatBody_Video() => videoQuote,
          _ => unsupportedText,
        },
        // So the reply bar previews a voice, picture or video original correctly
        // (message-content: Reply quote describes the original's kind).
        kind: switch (body) {
          ChatBody_Image() => ReplyKind.image,
          ChatBody_Audio() => ReplyKind.audio,
          ChatBody_Video() => ReplyKind.video,
          _ => ReplyKind.text,
        },
        durationMs: switch (body) {
          ChatBody_Audio(:final audio) => audio.durationMs,
          _ => 0,
        },
        thumbhash: switch (body) {
          ChatBody_Image(:final image) => image.thumbhash,
          _ => const [],
        },
      ),
    );
  }

  /// The text Copy puts on the clipboard for [message], or null when it has no
  /// text to copy (message-actions: Copy a message): a text body, or an image
  /// with a non-empty caption. Media-only messages, tombstones and unsupported
  /// bodies have none.
  String? _copyText(ChatMessage message) {
    final body = message.body;
    final text = switch (body) {
      ChatBody_Text(:final text) => text,
      ChatBody_Image(:final image) => image.caption,
      _ => null,
    };
    return (text != null && text.isNotEmpty) ? text : null;
  }

  /// The text Forward sends, or null when [message] is not forwardable
  /// (message-actions: Forward a message): only a text body this version; a
  /// forward of an attachment needs a core call this change does not add.
  String? _forwardText(ChatMessage message) {
    final body = message.body;
    return body is ChatBody_Text && body.text.isNotEmpty ? body.text : null;
  }

  /// Copies [message]'s text to the clipboard and confirms (message-actions:
  /// Copy a message). Nothing leaves the device.
  Future<void> _copyMessage(ChatMessage message) async {
    final text = _copyText(message);
    if (text == null) return;
    final messenger = ScaffoldMessenger.of(context);
    await Clipboard.setData(ClipboardData(text: text));
    messenger.showSnackBar(const SnackBar(content: Text('Copied')));
  }

  /// Forwards [message]'s text into a chat the user picks (message-actions:
  /// Forward a message): a new standalone message, carrying neither the
  /// original's mentions nor its reply quote.
  Future<void> _forwardMessage(ChatMessage message) async {
    final text = _forwardText(message);
    if (text == null) return;
    final messenger = ScaffoldMessenger.of(context);
    final target = await ForwardPicker.pick(
      context,
      widget.core,
      excludeChatId: _model.chatId,
    );
    if (target == null) return;
    try {
      if (target.chatId == 0) {
        await widget.core.sendText(text);
      } else {
        await widget.core.sendMessageIn(target.chatId, text);
      }
      messenger.showSnackBar(
        SnackBar(content: Text('Forwarded to ${target.title}')),
      );
    } catch (error) {
      messenger.showSnackBar(SnackBar(content: Text(errorText(error))));
    }
  }

  Future<void> _send(String text, List<MentionSpan> mentions) async {
    await _model.send(text, mentions: mentions, replyTo: _reply?.id);
    if (mounted) setState(() => _reply = null);
  }

  Future<void> _sendImage(
    Uint8List picked,
    String? caption,
    List<MentionSpan> mentions,
  ) async {
    await _model.sendImage(picked, caption: caption, mentions: mentions);
    // Picking a picture keeps the reply pending; sending it consumes the reply.
    if (mounted) setState(() => _reply = null);
  }

  Future<void> _sendSticker(Cutout cutout) => _model.sendSticker(cutout);

  Future<void> _sendAudio(Uint8List wav, {int? replyTo}) async {
    await _model.sendAudio(wav, replyTo: replyTo);
    if (mounted) setState(() => _reply = null);
  }

  Future<void> _resendAudio(ChatMessage message) async {
    final messenger = ScaffoldMessenger.of(context);
    try {
      await _model.resendAudio(message.id);
    } catch (error) {
      messenger.showSnackBar(SnackBar(content: Text(errorText(error))));
    }
  }

  Future<void> _resendImage(ChatMessage message) async {
    final messenger = ScaffoldMessenger.of(context);
    try {
      await _model.resendImage(message.id);
    } catch (error) {
      messenger.showSnackBar(SnackBar(content: Text(errorText(error))));
    }
  }

  Future<void> _deleteMessage(ChatMessage message) async {
    final messenger = ScaffoldMessenger.of(context);
    try {
      await _model.deleteMessage(message.id);
    } catch (error) {
      messenger.showSnackBar(SnackBar(content: Text(errorText(error))));
    }
  }

  /// Whether Report and Block are offered on [message] (moderation: Report a
  /// message; Block a user): only on another user's real message, never on an
  /// own message, a tombstone, or a report block itself.
  bool _canModerate(ChatMessage message) =>
      !message.ownUser &&
      message.body is! ChatBody_Deleted &&
      _reportText(message) == null;

  /// The report `Content.Text` of [message] if it is a report DM (moderation:
  /// The admin sees reports), else `null`; only a text body can be a report.
  String? _reportText(ChatMessage message) {
    final body = message.body;
    return body is ChatBody_Text && isReportText(body.text) ? body.text : null;
  }

  /// Reports [message] to the admin after asking for a short reason (moderation:
  /// Report a message; app-ui: Report a message or a user).
  Future<void> _reportMessage(ChatMessage message) async {
    final messenger = ScaffoldMessenger.of(context);
    final reason = await askReportReason(context);
    if (reason == null) return;
    try {
      await _model.report(message.id, reason);
      messenger.showSnackBar(const SnackBar(content: Text('Report sent')));
    } catch (error) {
      messenger.showSnackBar(SnackBar(content: Text(errorText(error))));
    }
  }

  /// Blocks [user] after a confirm that it hides them on this device only
  /// (moderation: Block a user; app-ui: Block and unblock a user).
  Future<void> _blockUser(UserRef user) async {
    final messenger = ScaffoldMessenger.of(context);
    final name = _model.names.of(user);
    if (!await confirmBlock(context, name)) return;
    try {
      await _model.block(user);
      messenger.showSnackBar(SnackBar(content: Text('Blocked $name')));
    } catch (error) {
      messenger.showSnackBar(SnackBar(content: Text(errorText(error))));
    }
  }

  /// Picks a source (record or gallery), normalizes the clip through the native
  /// plugin and sends it (app-ui: Send a video). Owned here so the composer
  /// stays free of the camera, picker and plugin.
  Future<void> _sendVideo() async {
    final source = VideoScope.of(context);
    final messenger = ScaffoldMessenger.of(context);
    final choice = await showModalBottomSheet<String>(
      context: context,
      builder: (sheet) => SafeArea(
        child: Column(
          key: const Key('video-source-sheet'),
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(
              key: const Key('video-record'),
              leading: const Icon(Icons.videocam),
              title: const Text('Record'),
              onTap: () => Navigator.of(sheet).pop('record'),
            ),
            ListTile(
              key: const Key('video-gallery'),
              leading: const Icon(Icons.video_library_outlined),
              title: const Text('Choose from gallery'),
              onTap: () => Navigator.of(sheet).pop('gallery'),
            ),
          ],
        ),
      ),
    );
    if (choice == null) return;
    try {
      final clip = choice == 'record'
          ? await source.record()
          : await source.pick();
      if (clip == null) return;
      await _model.sendVideo(
        clip.mp4,
        durationMs: clip.durationMs,
        width: clip.width,
        height: clip.height,
        firstFrameRgba: clip.firstFrameRgba,
        frameWidth: clip.frameWidth,
        frameHeight: clip.frameHeight,
      );
      // The reply bar goes once the video is sent, as for a text or voice send.
      if (mounted) setState(() => _reply = null);
    } catch (error) {
      messenger.showSnackBar(SnackBar(content: Text(errorText(error))));
    }
  }

  /// Whether this device may delete [message] (message-deletion: Who may
  /// delete): an admin in the lobby or a named group (`isGroup`), or the user's
  /// own message in a DM. Never a tombstone, and never once this device is
  /// removed.
  bool _canDelete(ChatMessage message, bool isGroup) {
    if (widget.core.removed.value || message.body is ChatBody_Deleted) {
      return false;
    }
    return isGroup ? widget.admin : message.ownUser;
  }

  /// Reveals the original of a DM tombstone and shows it in place.
  Future<void> _reveal(ChatMessage message) async {
    final messenger = ScaffoldMessenger.of(context);
    try {
      final original = await _model.reveal(message.id);
      if (mounted) setState(() => _revealed[message.id] = original);
    } catch (error) {
      messenger.showSnackBar(SnackBar(content: Text(errorText(error))));
    }
  }

  Future<void> _resendVideo(ChatMessage message) async {
    final messenger = ScaffoldMessenger.of(context);
    try {
      await _model.resendVideo(message.id);
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

  /// The app bar's second line: the peer's username in a DM, else how many
  /// members the group has (app-shell: Conversation screen).
  String _subtitle(UserRef? peer) {
    if (peer != null) return peer.username;
    final n = _model.memberUsers.length;
    return n == 0 ? '' : (n == 1 ? '1 member' : '$n members');
  }

  /// Back (the route's own), the avatar, the title with its subtitle, the
  /// connection indicator and the screen's [actions].
  AppBar _appBar({
    required String title,
    required String subtitle,
    required Widget avatar,
    required List<Widget> actions,
  }) => AppBar(
    titleSpacing: 0,
    title: Row(
      children: [
        avatar,
        const SizedBox(width: 12),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(title, maxLines: 1, overflow: TextOverflow.ellipsis),
              if (subtitle.isNotEmpty)
                Text(
                  subtitle,
                  key: const Key('conversation-subtitle'),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: Theme.of(context).textTheme.bodySmall,
                ),
            ],
          ),
        ),
      ],
    ),
    actions: [
      ConnectionIndicator(core: widget.core),
      ...actions,
    ],
  );

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
        appBar: _appBar(
          title: peer == null ? _model.title : peerName,
          subtitle: _subtitle(peer),
          // manual-presence: Presence indicator on avatars. A DM header shows
          // the peer's presence dot; the lobby and named groups show none.
          avatar: PresenceAvatar(
            key: const Key('conversation-avatar'),
            core: widget.core,
            user: peer,
            name: peer == null ? _model.title : peerName,
            seed: peer?.username ?? 'group-${_model.chatId}',
            radius: 18,
            icon: _model.isMain ? Icons.groups_outlined : null,
          ),
          actions: [
            _mentionBadge(),
            // A DM shows no admin actions (app-ui: DM screen); a named
            // group has its own menu (app-ui: Named group screen).
            if (_model.isGroup)
              GroupMenu(
                core: widget.core,
                chatId: _model.chatId,
                admin: widget.admin,
                names: _model.names,
                muted: _muted,
                onToggleMute: _toggleMuted,
              )
            else
              ChatMenu(
                admin: widget.admin && _model.isMain,
                lobby: _model.isMain,
                muted: _muted,
                onToggleMute: _toggleMuted,
              ),
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
                                child: switch (_reportText(message)) {
                                  final String t => ReportTile(
                                    core: widget.core,
                                    text: t,
                                  ),
                                  null => MessageTile(
                                  key: ValueKey('message-${message.id}'),
                                  message: message,
                                  sender: _model.names.of(message.sender),
                                  showSender: peer == null,
                                  mentionLabel: _mentionLabel,
                                  me: me,
                                  replyTo: switch (message.reply) {
                                    final ReplyQuote r => _model.names.of(r.to),
                                    null => null,
                                  },
                                  core: widget.core,
                                  onResendImage: () => _resendImage(message),
                                  onResendAudio: () => _resendAudio(message),
                                  onResendVideo: () => _resendVideo(message),
                                  playback: _playback,
                                  onReply: widget.core.removed.value
                                      ? null
                                      : () => _startReply(message),
                                  onCopy: _copyText(message) != null
                                      ? () => _copyMessage(message)
                                      : null,
                                  onForward: _forwardText(message) != null
                                      ? () => _forwardMessage(message)
                                      : null,
                                  onDelete:
                                      _canDelete(message, peer == null)
                                      ? () => _deleteMessage(message)
                                      : null,
                                  onReport:
                                      _canModerate(message)
                                      ? () => _reportMessage(message)
                                      : null,
                                  onBlock:
                                      _canModerate(message)
                                      ? () => _blockUser(message.sender)
                                      : null,
                                  onReveal: message.body is ChatBody_Deleted
                                      ? () => _reveal(message)
                                      : null,
                                  revealed: _revealed[message.id],
                                  onQuoteTap: switch (message.reply) {
                                    final ReplyQuote r => () => _openOriginal(
                                      r,
                                    ),
                                    null => null,
                                  },
                                ),
                                },
                              ),
                            ),
                            NoticeItem(:final notice) => NoticeTile(
                              notice: notice,
                              text: noticeText(context, notice, _name),
                              onAccept:
                                  notice is KeyChangedNotice &&
                                      !notice.acknowledged
                                  ? () => _model.acknowledge(notice)
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
            if (_model.closed && _model.isGroup)
              const MaterialBanner(
                key: Key('group-closed'),
                content: Text(groupClosedText),
                actions: [SizedBox.shrink()],
              )
            else if (_model.closed)
              MaterialBanner(
                key: const Key('dm-closed'),
                content: Text(dmClosedText(peerName)),
                actions: const [SizedBox.shrink()],
              )
            else ...[
              if (_model.outOfSync)
                MaterialBanner(
                  key: const Key('out-of-sync'),
                  content: const Text(outOfSyncText),
                  actions: [
                    TextButton(
                      key: const Key('re-sync'),
                      onPressed: () async {
                        final messenger = ScaffoldMessenger.of(context);
                        await widget.core.requestResync();
                        messenger.showSnackBar(
                          const SnackBar(content: Text('Re-syncing…')),
                        );
                      },
                      child: const Text('Re-sync'),
                    ),
                  ],
                ),
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
                segmenter: SegmenterScope.of(context),
                sendSticker: _sendSticker,
                sendAudio: _sendAudio,
                onSendVideo: _sendVideo,
                onRecordingStarted: () => _playback?.stop(),
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
          Expanded(child: SketchDivider(seed: 1, color: scheme.primary)),
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
          Expanded(child: SketchDivider(seed: 2, color: scheme.primary)),
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
  removeUser('Remove a user', Routes.removeUser);

  const MenuEntry(this.label, this.route);

  final String label;
  final String route;

  /// Whether only admin devices show it.
  bool get adminOnly => switch (this) {
    invite || invites || removeUser => true,
    members || profile || linkDevice => false,
  };
}

/// The chat's menu: everyone's entries, and the admin's only on admin
/// devices (the server rejects other devices' admin calls anyway).
/// The mute/unmute entry's value in [ChatMenu] (it is not a route like the
/// other entries).
const String _muteAction = '__mute_toggle__';

class ChatMenu extends StatelessWidget {
  const ChatMenu({
    super.key,
    required this.admin,
    this.lobby = false,
    this.muted = false,
    this.onToggleMute,
  });

  final bool admin;

  /// Whether this is the lobby's menu: it lists "Rename group" and "Close
  /// group" greyed out, so the fixed name and the missing close show
  /// (app-ui: Lobby is fixed; change `lobby-multi-group`).
  final bool lobby;

  /// Whether this conversation is muted (notification-prefs: Mute a
  /// conversation).
  final bool muted;

  /// Mutes or unmutes this conversation.
  final Future<void> Function()? onToggleMute;

  @override
  Widget build(BuildContext context) => PopupMenuButton<Object>(
    tooltip: 'Menu',
    onSelected: (entry) {
      if (entry == _muteAction) {
        onToggleMute?.call();
      } else if (entry is MenuEntry) {
        Navigator.of(context).pushNamed(entry.route);
      }
    },
    itemBuilder: (context) => [
      if (onToggleMute != null)
        PopupMenuItem<Object>(
          key: const Key('conversation-mute-toggle'),
          value: _muteAction,
          child: Text(muted ? 'Unmute notifications' : 'Mute notifications'),
        ),
      for (final entry in MenuEntry.values)
        if (admin || !entry.adminOnly)
          PopupMenuItem(value: entry, child: Text(entry.label)),
      if (lobby) ...const [
        PopupMenuDivider(),
        PopupMenuItem<Object>(
          key: ValueKey('lobby-rename-disabled'),
          enabled: false,
          child: Text('Rename group (the lobby is fixed)'),
        ),
        PopupMenuItem<Object>(
          key: ValueKey('lobby-close-disabled'),
          enabled: false,
          child: Text('Close group (the lobby cannot be closed)'),
        ),
      ],
    ],
  );
}
