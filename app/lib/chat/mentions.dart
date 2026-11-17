// app-ui: Mention picker; Mentions of me (change `mentions`, design D1, D2).
// Ranges are Dart string indices (UTF-16 code units), as the bridge's
// MentionSpan; the core converts them to UTF-8 bytes. Nothing here stores
// or logs text.
import 'package:flutter/material.dart';

import '../core/core_service.dart';

/// What "everyone" is called in the picker and in a drawn mention.
const everyoneLabel = 'everyone';

/// One thing the picker offers: a member, or everyone.
class MentionChoice {
  const MentionChoice.user(UserRef this.user, this.label, this.detail);
  const MentionChoice.everyone()
    : user = null,
      label = everyoneLabel,
      detail = 'Everyone in the group';

  /// The member; null for everyone.
  final UserRef? user;

  /// What is inserted after the "@": the display name, or the username.
  final String label;

  /// The second line: "Ali (alice)", or what everyone means.
  final String detail;

  MentionTarget get target => switch (user) {
    final UserRef user => MentionTarget.user(user: user),
    null => const MentionTarget.everyone(),
  };
}

/// The `@` query being typed at [cursor] in [text]: the text after an "@"
/// at the start or after whitespace, up to the cursor, with no whitespace
/// in it. Null when the cursor is not in one.
({int at, String query})? mentionQuery(String text, int cursor) {
  if (cursor < 0 || cursor > text.length) return null;
  for (var i = cursor - 1; i >= 0; i--) {
    final ch = text[i];
    if (ch.trim().isEmpty) return null;
    if (ch == '@') {
      if (i == 0 || text[i - 1].trim().isEmpty) {
        return (at: i, query: text.substring(i + 1, cursor));
      }
      return null;
    }
  }
  return null;
}

/// The choices whose label or username starts with [query] (case
/// insensitive), members first, then everyone.
List<MentionChoice> filterChoices(List<MentionChoice> all, String query) {
  final q = query.toLowerCase();
  bool fits(MentionChoice c) =>
      c.label.toLowerCase().startsWith(q) ||
      (c.user?.username.toLowerCase().startsWith(q) ?? false) ||
      c.label.toLowerCase().split(' ').any((w) => w.startsWith(q));
  return [
    for (final c in all)
      if (fits(c)) c,
  ];
}

/// The mentions in a composer's text, kept in step with edits: an edit
/// before a mention moves it, an edit that touches it drops it (app-ui:
/// Mention picker, "Name edited away").
class ComposerMentions {
  final List<MentionSpan> _spans = [];
  String _text = '';

  /// The mentions, in text order.
  List<MentionSpan> get spans => List.unmodifiable(_spans);

  /// The text changed from what it was to [text].
  void edited(String text) {
    final old = _text;
    _text = text;
    if (_spans.isEmpty || old == text) return;
    var prefix = 0;
    final shorter = old.length < text.length ? old.length : text.length;
    while (prefix < shorter &&
        old.codeUnitAt(prefix) == text.codeUnitAt(prefix)) {
      prefix++;
    }
    var suffix = 0;
    while (suffix < shorter - prefix &&
        old.codeUnitAt(old.length - 1 - suffix) ==
            text.codeUnitAt(text.length - 1 - suffix)) {
      suffix++;
    }
    final editStart = prefix;
    final editEnd = old.length - suffix;
    final delta = text.length - old.length;
    final kept = <MentionSpan>[];
    for (final span in _spans) {
      final end = span.start + span.length;
      if (end <= editStart) {
        kept.add(span);
      } else if (span.start >= editEnd) {
        kept.add(
          MentionSpan(
            target: span.target,
            start: span.start + delta,
            length: span.length,
          ),
        );
      }
    }
    _spans
      ..clear()
      ..addAll(kept);
  }

  /// [choice] was picked for the query starting at [at] in [text], with the
  /// cursor at [cursor]: the new text and cursor, with the mention recorded.
  ({String text, int cursor}) insert(
    String text,
    int at,
    int cursor,
    MentionChoice choice,
  ) {
    final name = '@${choice.label}';
    final newText = '${text.substring(0, at)}$name ${text.substring(cursor)}';
    _text = text;
    edited(newText);
    _spans
      ..add(MentionSpan(target: choice.target, start: at, length: name.length))
      ..sort((a, b) => a.start.compareTo(b.start));
    return (text: newText, cursor: at + name.length + 1);
  }

  /// Forgets every mention (after a send).
  void clear() {
    _spans.clear();
    _text = '';
  }
}

/// Whether [target] names [me] or everyone.
bool namesMe(MentionTarget target, UserRef? me) => switch (target) {
  MentionTarget_Everyone() => true,
  MentionTarget_User(:final user) =>
    me != null && user.username == me.username && user.serverId == me.serverId,
};

/// [text] with each of [mentions] drawn as "@" + [label] of its target
/// (the reader's name for it, design D2), mentions of [me] and everyone
/// highlighted more strongly. Ranges that do not fit are drawn as text.
TextSpan mentionSpans(
  BuildContext context,
  String text,
  List<MentionSpan> mentions, {
  required String Function(MentionTarget target) label,
  required UserRef? me,
  bool own = false,
}) {
  final scheme = Theme.of(context).colorScheme;
  final children = <InlineSpan>[];
  var at = 0;
  final sorted = [...mentions]..sort((a, b) => a.start.compareTo(b.start));
  for (final m in sorted) {
    final end = m.start + m.length;
    if (m.start < at || end > text.length || m.length <= 0) continue;
    if (m.start > at) children.add(TextSpan(text: text.substring(at, m.start)));
    final mine = !own && namesMe(m.target, me);
    children.add(
      TextSpan(
        text: '@${label(m.target)}',
        style: mine
            ? TextStyle(
                color: scheme.onTertiaryContainer,
                backgroundColor: scheme.tertiaryContainer,
                fontWeight: FontWeight.bold,
              )
            : TextStyle(color: scheme.primary, fontWeight: FontWeight.w600),
        semanticsLabel: mine ? 'mention of you' : null,
      ),
    );
    at = end;
  }
  if (at < text.length) children.add(TextSpan(text: text.substring(at)));
  return TextSpan(children: children);
}
