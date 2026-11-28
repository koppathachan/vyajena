// app-ui: Type-aware notification wording; Notification detail level. The one
// place the words of a local notification are made, from the core's summary
// (type and unread count, never a sender or text), so iOS and Android show the
// same thing: Android posts the lines, iOS's Notification Service Extension
// shows the combined line the app saved (local-notifications: The device builds
// the notification locally).
//
// Basic shows only the generic "New message" (no count, no type, no name); More
// shows the per-conversation count and type. At neither level does anything here
// take or show a sender or a message body: the summary has none to give.
import '../core/core_service.dart';

/// The generic content-free notification (message-delivery: Wake push payload;
/// app-ui: Notification detail level, Basic).
const kGenericNotification = 'New message';

/// One notification line: a conversation's unread, or one admin-event.
class NotificationLine {
  const NotificationLine({required this.key, required this.text, this.chatId});

  /// Stable id of what this line is about, so a repeat replaces it: `chat-<id>`
  /// for a conversation (the OS groups by it), `event-<id>` for an admin-event,
  /// `basic` for the generic notification.
  final String key;

  /// The words shown.
  final String text;

  /// The conversation a tap opens; null opens the chat list.
  final int? chatId;

  @override
  bool operator ==(Object other) =>
      other is NotificationLine &&
      other.key == key &&
      other.text == text &&
      other.chatId == chatId;

  @override
  int get hashCode => Object.hash(key, text, chatId);

  @override
  String toString() => 'NotificationLine($key, $text, $chatId)';
}

/// What to show for a summary at a detail level.
class RenderedNotifications {
  const RenderedNotifications({
    required this.lines,
    required this.combined,
    required this.badge,
    this.singleChatId,
  });

  /// Nothing to show.
  static const empty = RenderedNotifications(lines: [], combined: '', badge: 0);

  /// One line per conversation or admin-event (the Android group's children);
  /// at Basic, the one generic line.
  final List<NotificationLine> lines;

  /// All of them on one line (the Android group summary; what iOS shows).
  final String combined;

  /// The app-icon badge: the unread total at More, nothing at Basic (a number
  /// would say more than "New message").
  final int badge;

  /// The one conversation a tap opens when exactly one is involved, else null.
  final int? singleChatId;

  bool get isEmpty => lines.isEmpty;
}

/// "3 new in Work", "2 new messages" (a DM, naming no one), "5 new in the lobby".
String conversationText(ConversationNotification c) => switch (c.kind) {
  NotificationConversationKind.dm =>
    c.unread == 1 ? '1 new message' : '${c.unread} new messages',
  NotificationConversationKind.namedGroup =>
    '${c.unread} new in ${_groupName(c.name)}',
  NotificationConversationKind.lobby => '${c.unread} new in the lobby',
};

/// The fixed phrase of an admin-event, never derived from any message.
String adminEventText(AdminEventNotification e) {
  final group = e.kind == NotificationConversationKind.dm
      ? 'a chat'
      : _groupName(e.name);
  return switch (e.event) {
    AdminEventType.removed => 'You were removed from $group',
    AdminEventType.closed =>
      e.kind == NotificationConversationKind.dm
          ? 'A chat was closed'
          : '${_capitalized(group)} was closed',
    AdminEventType.added => 'You were added to ${_addedGroup(e.name)}',
  };
}

String _groupName(String? name) =>
    (name == null || name.trim().isEmpty) ? 'a group' : name;

String _addedGroup(String? name) =>
    (name == null || name.trim().isEmpty) ? 'a new group' : name;

String _capitalized(String text) =>
    text.isEmpty ? text : text[0].toUpperCase() + text.substring(1);

/// The words for [summary]: one line per conversation with unread messages and
/// per admin-event, with the per-conversation count and type — never a sender or
/// any message text. Empty when there is nothing to notify about.
RenderedNotifications renderNotifications(NotificationSummaryView summary) {
  if (summary.conversations.isEmpty && summary.adminEvents.isEmpty) {
    return RenderedNotifications.empty;
  }
  final lines = <NotificationLine>[
    for (final c in summary.conversations)
      NotificationLine(
        key: 'chat-${c.chatId}',
        text: conversationText(c),
        chatId: c.chatId,
      ),
    for (final e in summary.adminEvents)
      NotificationLine(
        key: 'event-${e.id}',
        text: adminEventText(e),
        chatId: e.chatId,
      ),
  ];
  // The combined line counts DMs together ("2 new DMs") and keeps the rest one
  // by one.
  final dmUnread = summary.conversations
      .where((c) => c.kind == NotificationConversationKind.dm)
      .fold<int>(0, (sum, c) => sum + c.unread);
  final parts = <String>[
    if (dmUnread > 0) dmUnread == 1 ? '1 new DM' : '$dmUnread new DMs',
    for (final c in summary.conversations)
      if (c.kind != NotificationConversationKind.dm) conversationText(c),
    for (final e in summary.adminEvents) adminEventText(e),
  ];
  final targets = {for (final l in lines) l.chatId};
  return RenderedNotifications(
    lines: lines,
    combined: parts.join(', '),
    badge: summary.totalUnread,
    singleChatId: targets.length == 1 ? targets.first : null,
  );
}
