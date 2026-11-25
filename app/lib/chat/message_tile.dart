// app-ui: Chat screen. One message. Another user's message shows its sender
// as "Ali (alice)" and sits on the left. A message from this user (any of
// their devices, `ownUser`) sits on the right, sized to its text, with no
// sender label; one this device sent (`outgoing`) also shows its send
// status inline at the end of the bubble: ⏳ sending, ✓ accepted, or failed
// with the reason (qa-fixes design D1). A body this version does not know
// shows a placeholder.
//
// app-ui: Mentions of me; Reply to a message (change `mentions`). Mentions
// are drawn with the reader's name for their target, mentions of this user
// highlighted. A reply shows who and what it answers above its text; the
// quote is tappable. Swiping right or long-pressing offers Reply.
//
// app-ui: Play a voice message. A voice message is a bubble with a play button
// and the waveform (audio_bubble.dart).
import 'dart:typed_data';

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';

import '../core/core_service.dart';
import 'audio_bubble.dart';
import 'emoji.dart';
import 'image_bubble.dart';
import 'links.dart';
import 'mentions.dart';
import 'time_text.dart';
import 'video_bubble.dart';
import 'voice_playback.dart';

/// Placeholder for a message body this version does not know.
const unsupportedText =
    'This message needs a newer version of the app to be shown.';

/// The status mark of an own message.
String statusText(SendState status) => switch (status) {
  SendState_Sending() => '⏳',
  SendState_Accepted() => '✓',
  SendState_Failed(:final reason) => 'Failed: $reason',
};

/// What the long-press sheet's reply entry says.
const replyLabel = 'Reply';

/// What a reply to a picture without a caption quotes.
const pictureQuote = 'Picture';

/// What a reply to a voice message quotes.
const voiceQuote = 'Voice message';

/// What the long-press sheet's entry for sending a picture again says.
const resendPictureLabel = 'Resend picture';

/// What the long-press sheet's entry for sending a voice message again says.
const resendVoiceLabel = 'Resend voice message';

/// What the long-press sheet's entry for sending a video again says.
const resendVideoLabel = 'Resend video';

/// What a reply to a video quotes.
const videoQuote = 'Video';

/// What the long-press sheet's Delete entry says (message-deletion).
const deleteLabel = 'Delete';

/// The tombstone of a deleted message (message-deletion: Tombstone, not
/// erasure).
const deletedText = 'This message was deleted';

/// The tombstone of a message an admin deleted in a group.
const deletedByAdminText = 'This message was deleted by the admin';

/// What a reply to a deleted message quotes (message-deletion: Deleting media
/// and replies).
const deletedQuote = 'deleted message';

/// What the Reveal affordance on a DM tombstone says (message-deletion:
/// Receiver may reveal in a DM).
const revealLabel = 'Reveal';

/// One message row.
class MessageTile extends StatelessWidget {
  const MessageTile({
    super.key,
    required this.message,
    required this.sender,
    this.mentionLabel,
    this.me,
    this.replyTo,
    this.onReply,
    this.onQuoteTap,
    this.core,
    this.onResendImage,
    this.onResendAudio,
    this.onResendVideo,
    this.onDelete,
    this.onReveal,
    this.revealed,
    this.playback,
    this.showSender = true,
  });

  final ChatMessage message;

  /// Whether another user's bubble names its sender: in the lobby and named
  /// groups only, never in a DM (app-shell: Conversation screen).
  final bool showSender;

  /// How to show the sender, "Ali (alice)".
  final String sender;

  /// The name drawn after a mention's "@"; the username when unset.
  final String Function(MentionTarget target)? mentionLabel;

  /// This device's user, whose mentions are highlighted.
  final UserRef? me;

  /// How to show the user a reply answers.
  final String? replyTo;

  /// Starts a reply to this message (swipe right, or long-press → Reply).
  final VoidCallback? onReply;

  /// The reply's quote was tapped.
  final VoidCallback? onQuoteTap;

  /// The core, which an image message needs to load its picture.
  final CoreService? core;

  /// Resends an own picture as a new message (long-press → "Resend
  /// picture").
  final VoidCallback? onResendImage;

  /// Resends an own voice message as a new message (long-press → "Resend
  /// voice message").
  final VoidCallback? onResendAudio;

  /// Resends an own video as a new message (long-press → "Resend video").
  final VoidCallback? onResendVideo;

  /// Deletes this message (long-press → Delete), when this device may: an admin
  /// device in the lobby or a named group (any message), or the user's own
  /// message in a DM. The sheet shows Delete only when this is set
  /// (message-deletion: Who may delete).
  final VoidCallback? onDelete;

  /// Reveals the original of a DM tombstone this device still holds (the Reveal
  /// affordance; message-deletion: Receiver may reveal in a DM).
  final VoidCallback? onReveal;

  /// The revealed original of a deleted message, once the user tapped Reveal:
  /// its body is shown in place of the tombstone.
  final ChatMessage? revealed;

  /// The chat's one player, which a voice message plays through.
  final VoicePlayback? playback;

  String _label(MentionTarget target) =>
      mentionLabel?.call(target) ??
      switch (target) {
        MentionTarget_User(:final user) => user.username,
        MentionTarget_Everyone() => everyoneLabel,
      };

  Future<void> _sheet(BuildContext context) async {
    final resend = message.outgoing && message.body is ChatBody_Image
        ? onResendImage
        : null;
    final resendVoice = message.outgoing && message.body is ChatBody_Audio
        ? onResendAudio
        : null;
    final resendVideo = message.outgoing && message.body is ChatBody_Video
        ? onResendVideo
        : null;
    final choice = await showModalBottomSheet<String>(
      context: context,
      builder: (context) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(
              leading: const Icon(Icons.reply),
              title: const Text(replyLabel),
              onTap: () => Navigator.of(context).pop('reply'),
            ),
            if (resend != null)
              ListTile(
                key: const Key('resend-picture'),
                leading: const Icon(Icons.refresh),
                title: const Text(resendPictureLabel),
                onTap: () => Navigator.of(context).pop('resend'),
              ),
            if (resendVoice != null)
              ListTile(
                key: const Key('resend-voice'),
                leading: const Icon(Icons.refresh),
                title: const Text(resendVoiceLabel),
                onTap: () => Navigator.of(context).pop('resend-voice'),
              ),
            if (resendVideo != null)
              ListTile(
                key: const Key('resend-video'),
                leading: const Icon(Icons.refresh),
                title: const Text(resendVideoLabel),
                onTap: () => Navigator.of(context).pop('resend-video'),
              ),
            if (onDelete != null)
              ListTile(
                key: const Key('delete'),
                leading: const Icon(Icons.delete_outline),
                title: const Text(deleteLabel),
                onTap: () => Navigator.of(context).pop('delete'),
              ),
          ],
        ),
      ),
    );
    switch (choice) {
      case 'reply':
        onReply?.call();
      case 'resend':
        resend?.call();
      case 'resend-voice':
        resendVoice?.call();
      case 'resend-video':
        resendVideo?.call();
      case 'delete':
        onDelete?.call();
    }
  }

  /// A plain text body: drawn large when it is only a few emoji ("jumbomoji",
  /// see emoji.dart), else as ordinary text with its mentions and links.
  Widget _text(BuildContext context, String text) {
    final jumbo = jumboEmojiSize(text);
    if (jumbo != null) {
      return Text(
        text,
        key: ValueKey('jumbo-${message.id}'),
        style: TextStyle(fontSize: jumbo, height: 1.15),
      );
    }
    return _withMentions(context, text);
  }

  /// A tombstone for a deleted message (message-deletion: Tombstone, not
  /// erasure): the italic "This message was deleted" (with "by the admin" for a
  /// moderator delete), and in a DM, when this device still holds the original,
  /// a Reveal affordance (Receiver may reveal in a DM).
  Widget _tombstone(BuildContext context, bool byAdmin, bool revealable) {
    final scheme = Theme.of(context).colorScheme;
    final label = Text(
      byAdmin ? deletedByAdminText : deletedText,
      style: TextStyle(
        fontStyle: FontStyle.italic,
        color: scheme.onSurfaceVariant,
      ),
    );
    if (!revealable || onReveal == null) {
      return label;
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        label,
        TextButton(
          key: const Key('reveal'),
          onPressed: onReveal,
          style: TextButton.styleFrom(
            padding: EdgeInsets.zero,
            minimumSize: const Size(0, 0),
            tapTargetSize: MaterialTapTargetSize.shrinkWrap,
          ),
          child: const Text(revealLabel),
        ),
      ],
    );
  }

  /// [text] with this message's mentions and its links drawn in it.
  Widget _withMentions(BuildContext context, String text) => _MessageText(
    text: text,
    mentions: message.mentions,
    label: _label,
    me: me,
    own: message.ownUser,
  );

  Widget _image(BuildContext context, ImageView image) {
    final core = this.core;
    final caption = image.caption;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        if (core == null)
          const Text('Picture')
        else
          ImageBubble(
            key: ValueKey('image-${message.id}'),
            core: core,
            messageId: message.id,
            image: image,
          ),
        if (caption != null && caption.isNotEmpty) ...[
          const SizedBox(height: 6),
          _withMentions(context, caption),
        ],
      ],
    );
  }

  Widget _audio(BuildContext context, AudioView audio) {
    final core = this.core;
    final playback = this.playback;
    if (core == null || playback == null) return const Text('Voice message');
    return AudioBubble(
      key: ValueKey('audio-${message.id}'),
      core: core,
      messageId: message.id,
      audio: audio,
      playback: playback,
    );
  }

  Widget _video(BuildContext context, VideoView video) {
    final core = this.core;
    if (core == null) return const Text('Video');
    return VideoBubble(
      key: ValueKey('video-${message.id}'),
      core: core,
      messageId: message.id,
      video: video,
    );
  }

  Widget _quote(BuildContext context, ReplyQuote reply) {
    final scheme = Theme.of(context).colorScheme;
    final text = Theme.of(context).textTheme;
    return InkWell(
      key: ValueKey('quote-${message.id}'),
      onTap: onQuoteTap,
      child: Container(
        margin: const EdgeInsets.only(bottom: 4),
        padding: const EdgeInsets.fromLTRB(8, 4, 8, 4),
        decoration: BoxDecoration(
          border: Border(left: BorderSide(color: scheme.primary, width: 3)),
          color: scheme.surfaceContainerHighest,
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              replyTo ?? reply.to.username,
              style: text.labelSmall?.copyWith(color: scheme.primary),
            ),
            _quoteBody(context, reply),
          ],
        ),
      ),
    );
  }

  /// The quoted original's preview, driven by its kind so it renders even when
  /// this device does not hold the original (message-content: Reply quote
  /// describes the original's kind): a voice original as a mic glyph and the
  /// duration, a captionless picture as a picture glyph, and text as its quote.
  Widget _quoteBody(BuildContext context, ReplyQuote reply) {
    final scheme = Theme.of(context).colorScheme;
    final style = Theme.of(
      context,
    ).textTheme.bodySmall?.copyWith(color: scheme.onSurfaceVariant);
    // message-deletion: Deleting media and replies. A reply to a message this
    // device knows was deleted quotes "deleted message", not the stale snapshot.
    if (reply.originalDeleted) {
      return Text(
        deletedQuote,
        style: style?.copyWith(fontStyle: FontStyle.italic),
      );
    }
    switch (reply.kind) {
      case ReplyKind.audio:
        final length = clipLengthText(Duration(milliseconds: reply.durationMs));
        return Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.mic, size: 16, color: scheme.onSurfaceVariant),
            const SizedBox(width: 4),
            Text('$voiceQuote · $length', style: style),
          ],
        );
      case ReplyKind.image:
        final c = core;
        final blur = (reply.thumbhash.isEmpty || c == null)
            ? null
            : c.thumbhashPreview(Uint8List.fromList(reply.thumbhash));
        return Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (blur != null)
              ClipRRect(
                borderRadius: BorderRadius.circular(3),
                child: Image.memory(
                  blur,
                  width: 28,
                  height: 28,
                  fit: BoxFit.cover,
                ),
              )
            else
              Icon(Icons.photo, size: 16, color: scheme.onSurfaceVariant),
            const SizedBox(width: 4),
            Flexible(
              child: Text(
                reply.quote.isEmpty ? pictureQuote : reply.quote,
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                style: style,
              ),
            ),
          ],
        );
      case ReplyKind.text:
        return Text(
          reply.quote.isEmpty ? unsupportedText : reply.quote,
          maxLines: 2,
          overflow: TextOverflow.ellipsis,
          style: style,
        );
    }
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final text = Theme.of(context).textTheme;
    final own = message.ownUser;
    final status = message.outgoing ? message.status : null;
    final sentMs = (message.sentAtMs ?? message.acceptedAtMs)?.toInt();
    // A revealed tombstone shows the original body in place of the tombstone.
    final displayBody = message.body is ChatBody_Deleted && revealed != null
        ? revealed!.body
        : message.body;
    final textBody = switch (displayBody) {
      ChatBody_Text(:final text) => _text(context, text),
      ChatBody_Image(:final image) => _image(context, image),
      ChatBody_Audio(:final audio) => _audio(context, audio),
      ChatBody_Video(:final video) => _video(context, video),
      ChatBody_Deleted(:final byAdmin, :final revealable) => _tombstone(
        context,
        byAdmin,
        revealable,
      ),
      ChatBody_Unsupported() => Text(
        unsupportedText,
        style: TextStyle(
          fontStyle: FontStyle.italic,
          color: scheme.onSurfaceVariant,
        ),
      ),
    };
    final reply = message.reply;
    final body = reply == null
        ? textBody
        : Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [_quote(context, reply), textBody],
          );
    // Everything here sizes to its content: a widget that fills the width
    // it is given (an Align, say) would stretch the bubble across the
    // screen (design D1).
    final content = own
        ? body
        : Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              if (showSender) ...[
                Text(
                  sender,
                  style: text.labelMedium?.copyWith(color: scheme.primary),
                ),
                const SizedBox(height: 2),
              ],
              body,
            ],
          );
    final bubble = Align(
      alignment: own ? Alignment.centerRight : Alignment.centerLeft,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 480),
        child: Card(
          color: own ? scheme.secondaryContainer : scheme.surfaceContainerHigh,
          elevation: 0,
          shape: message.mentionsMe
              ? RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(12),
                  side: BorderSide(color: scheme.tertiary, width: 1.5),
                )
              : null,
          margin: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(12, 8, 12, 6),
            child: IntrinsicWidth(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  content,
                  if (sentMs != null || status != null) ...[
                    const SizedBox(height: 2),
                    // The time and the delivery tick sit on one right-aligned
                    // line below the body. A MainAxisSize.max Row forwards its
                    // intrinsic width, so the bubble widens to keep both on the
                    // line; the status is Flexible so a long failure reason
                    // wraps rather than overflowing the bubble.
                    Row(
                      mainAxisAlignment: MainAxisAlignment.end,
                      crossAxisAlignment: CrossAxisAlignment.center,
                      children: [
                        if (sentMs != null)
                          Text(
                            clockText(context, sentMs),
                            key: ValueKey('time-${message.id}'),
                            style: text.labelSmall?.copyWith(
                              color: scheme.onSurfaceVariant.withValues(
                                alpha: 0.6,
                              ),
                            ),
                          ),
                        if (sentMs != null && status != null)
                          const SizedBox(width: 6),
                        if (status != null)
                          Flexible(
                            child: Text(
                              statusText(status),
                              style: text.labelSmall?.copyWith(
                                color: status is SendState_Failed
                                    ? scheme.error
                                    : scheme.onSurfaceVariant.withValues(
                                        alpha: 0.6,
                                      ),
                              ),
                            ),
                          ),
                      ],
                    ),
                  ],
                ],
              ),
            ),
          ),
        ),
      ),
    );
    final startReply = onReply;
    if (startReply == null) return bubble;
    // Swipe right to reply: the row slides, then springs back (it is never
    // dismissed).
    return Dismissible(
      key: ValueKey('swipe-${message.id}'),
      direction: DismissDirection.startToEnd,
      dismissThresholds: const {DismissDirection.startToEnd: 0.12},
      confirmDismiss: (_) async {
        startReply();
        return false;
      },
      background: Align(
        alignment: Alignment.centerLeft,
        child: Padding(
          padding: const EdgeInsets.only(left: 16),
          child: Icon(Icons.reply, color: scheme.primary),
        ),
      ),
      child: GestureDetector(onLongPress: () => _sheet(context), child: bubble),
    );
  }
}

/// Message text: plain when it has neither mentions nor links, else rich with
/// mentions drawn and URLs tappable (app-ui: Links in messages). Owns the
/// links' tap recognizers.
class _MessageText extends StatefulWidget {
  const _MessageText({
    required this.text,
    required this.mentions,
    required this.label,
    required this.me,
    required this.own,
  });

  final String text;
  final List<MentionSpan> mentions;
  final String Function(MentionTarget target) label;
  final UserRef? me;
  final bool own;

  @override
  State<_MessageText> createState() => _MessageTextState();
}

class _MessageTextState extends State<_MessageText> {
  final List<GestureRecognizer> _recognizers = [];

  void _dispose() {
    for (final r in _recognizers) {
      r.dispose();
    }
    _recognizers.clear();
  }

  @override
  void dispose() {
    _dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    _dispose();
    if (widget.mentions.isEmpty && findLinks(widget.text).isEmpty) {
      return Text(widget.text);
    }
    return Text.rich(
      mentionSpans(
        context,
        widget.text,
        widget.mentions,
        label: widget.label,
        me: widget.me,
        own: widget.own,
        onLink: (url) => showLinkSheet(context, url),
        recognizers: _recognizers,
      ),
    );
  }
}
