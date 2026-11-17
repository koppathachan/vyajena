// app-ui: Composer limit. Text whose encoded content is over the core's
// MAX_CONTENT_BYTES is refused before sending (`textFits`); the text stays
// in the composer. Once this device was removed from the group the
// composer is disabled (app-ui: Notices in the chat, "Bob is removed").
//
// app-ui: Mention picker. Typing "@" at the start or after a space lists the
// other members and everyone; picking one inserts "@" + its name and
// records the mention by user id. app-ui: Reply to a message: while
// replying, the quote shows above the box with a cancel button.
import 'package:flutter/material.dart';

import '../app/errors.dart';
import '../core/core_service.dart';
import 'mentions.dart';

/// The reply the composer is writing: who and what it answers.
class ReplyDraft {
  const ReplyDraft({required this.id, required this.name, required this.quote});

  /// The original's local id.
  final int id;

  /// How to show its sender.
  final String name;

  /// Its text, cut for the bar.
  final String quote;
}

/// What the composer says about text that does not fit.
const tooLongText = 'Too long to send. Make it shorter.';

/// The message box and Send button.
class Composer extends StatefulWidget {
  const Composer({
    super.key,
    required this.core,
    required this.send,
    this.choices,
    this.reply,
    this.onCancelReply,
  });

  final CoreService core;

  /// Sends the text with its mentions (stores it in the core).
  final Future<void> Function(String text, List<MentionSpan> mentions) send;

  /// What the mention picker offers; no picker when unset.
  final List<MentionChoice> Function()? choices;

  /// The reply being written, if any.
  final ReplyDraft? reply;

  /// Drops [reply].
  final VoidCallback? onCancelReply;

  @override
  State<Composer> createState() => _ComposerState();
}

class _ComposerState extends State<Composer> {
  final _text = TextEditingController();
  final _mentions = ComposerMentions();
  String? _error;
  bool _sending = false;
  ({int at, String query})? _query;

  void _changed(String text) {
    _mentions.edited(text);
    final cursor = _text.selection.baseOffset;
    final query = widget.choices == null
        ? null
        : mentionQuery(text, cursor < 0 ? text.length : cursor);
    setState(() {
      _query = query;
      _error = null;
    });
  }

  void _pick(MentionChoice choice) {
    final query = _query;
    if (query == null) return;
    final cursor = _text.selection.baseOffset < 0
        ? _text.text.length
        : _text.selection.baseOffset;
    final result = _mentions.insert(_text.text, query.at, cursor, choice);
    _text.value = TextEditingValue(
      text: result.text,
      selection: TextSelection.collapsed(offset: result.cursor),
    );
    setState(() => _query = null);
  }

  Widget _picker(BuildContext context) {
    final query = _query;
    final all = widget.choices;
    if (query == null || all == null) return const SizedBox.shrink();
    final choices = filterChoices(all(), query.query);
    if (choices.isEmpty) return const SizedBox.shrink();
    return ConstrainedBox(
      key: const Key('mention-picker'),
      constraints: const BoxConstraints(maxHeight: 220),
      child: Material(
        color: Theme.of(context).colorScheme.surfaceContainer,
        child: ListView(
          shrinkWrap: true,
          padding: EdgeInsets.zero,
          children: [
            for (final c in choices)
              ListTile(
                dense: true,
                leading: Icon(
                  c.user == null ? Icons.groups : Icons.alternate_email,
                ),
                title: Text(c.label),
                subtitle: Text(c.detail),
                onTap: () => _pick(c),
              ),
          ],
        ),
      ),
    );
  }

  Widget _replyBar(BuildContext context, ReplyDraft reply) {
    final scheme = Theme.of(context).colorScheme;
    final text = Theme.of(context).textTheme;
    return Container(
      key: const Key('reply-bar'),
      margin: const EdgeInsets.fromLTRB(12, 4, 8, 0),
      padding: const EdgeInsets.only(left: 8),
      decoration: BoxDecoration(
        border: Border(left: BorderSide(color: scheme.primary, width: 3)),
      ),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  'Replying to ${reply.name}',
                  style: text.labelSmall?.copyWith(color: scheme.primary),
                ),
                Text(
                  reply.quote,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: text.bodySmall,
                ),
              ],
            ),
          ),
          IconButton(
            tooltip: 'Cancel reply',
            onPressed: widget.onCancelReply,
            icon: const Icon(Icons.close),
          ),
        ],
      ),
    );
  }

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  Future<void> _send() async {
    final text = _text.text;
    if (text.trim().isEmpty || _sending) return;
    if (!widget.core.textFits(text)) {
      setState(() => _error = tooLongText);
      return;
    }
    setState(() {
      _error = null;
      _sending = true;
    });
    try {
      await widget.send(text, _mentions.spans);
      _text.clear();
      _mentions.clear();
      if (mounted) setState(() => _query = null);
    } catch (error) {
      if (mounted) setState(() => _error = errorText(error));
    } finally {
      if (mounted) setState(() => _sending = false);
    }
  }

  @override
  Widget build(BuildContext context) => ValueListenableBuilder<bool>(
    valueListenable: widget.core.removed,
    builder: (context, removed, _) => _box(context, removed),
  );

  Widget _box(BuildContext context, bool removed) => SafeArea(
    top: false,
    child: Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        _picker(context),
        if (widget.reply case final ReplyDraft reply when !removed)
          _replyBar(context, reply),
        _field(context, removed),
      ],
    ),
  );

  Widget _field(BuildContext context, bool removed) => Padding(
    padding: const EdgeInsets.fromLTRB(12, 4, 8, 8),
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.end,
      children: [
        Expanded(
          child: TextField(
            key: const Key('composer'),
            controller: _text,
            enabled: !removed,
            minLines: 1,
            maxLines: 6,
            textCapitalization: TextCapitalization.sentences,
            decoration: InputDecoration(
              hintText: removed ? 'Removed from the group' : 'Message',
              border: const OutlineInputBorder(),
              isDense: true,
              errorText: _error,
              errorMaxLines: 3,
            ),
            onChanged: _changed,
          ),
        ),
        const SizedBox(width: 4),
        IconButton.filled(
          tooltip: 'Send',
          onPressed: _sending || removed ? null : _send,
          icon: const Icon(Icons.send),
        ),
      ],
    ),
  );
}
