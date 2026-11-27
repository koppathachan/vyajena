// app-ui: Notification detail level; Type-aware notification wording; A
// notification opens its conversation. local-notifications: Respect the
// notification detail level (change `smarter-notifications`). Over the fake core
// and a fake phone; nothing here has a sender or a message body to show.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/app/app.dart';
import 'package:vyajena/core/core_service.dart';
import 'package:vyajena/push/notification_service.dart';
import 'package:vyajena/push/notification_text.dart';
import 'package:vyajena/push/push_platform.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';
import '../support/fake_push.dart';

ConversationNotification group(int id, String? name, int unread) =>
    ConversationNotification(
      chatId: id,
      kind: NotificationConversationKind.namedGroup,
      name: name,
      unread: unread,
    );

ConversationNotification dm(int id, int unread) => ConversationNotification(
  chatId: id,
  kind: NotificationConversationKind.dm,
  unread: unread,
);

NotificationSummaryView summary(
  List<ConversationNotification> conversations, [
  List<AdminEventNotification> events = const [],
]) => NotificationSummaryView(
  conversations: conversations,
  adminEvents: events,
  totalUnread: conversations.fold(0, (sum, c) => sum + c.unread),
);

void connect(FakeCoreService core) =>
    core.emit(const CoreEvent.connection(status: ConnectionStatus.connected));

Future<(FakeCoreService, FakePush, NotificationService)> woken({
  required NotificationSummaryView summary,
  NotificationDetailLevel level = NotificationDetailLevel.more,
  bool foreground = false,
}) async {
  final core = FakeCoreService(storedUser: bob)
    ..summaryValue = summary
    ..detailValue = level;
  await core.open();
  // The app went to the background and stopped the core.
  await core.stop();
  core.calls.clear();
  core.onStart = () => connect(core);
  final push = FakePush();
  final service = NotificationService(
    core: core,
    api: push,
    isForeground: () => foreground,
    quiet: const Duration(milliseconds: 20),
    limit: const Duration(seconds: 2),
  )..start();
  return (core, push, service);
}

void main() {
  // spec: app-ui/type-aware-notification-wording
  test('Bob at More: a group names itself, a DM names no one, the lobby is '
      '"the lobby"', () {
    final shown = renderNotifications(
      summary([group(100, 'Work', 3), dm(1, 2)]),
      NotificationDetailLevel.more,
    );
    expect(shown.lines.map((l) => l.text), ['3 new in Work', '2 new messages']);
    expect(shown.lines.map((l) => l.key), ['chat-100', 'chat-1']);
    expect(shown.badge, 5);
    expect(
      conversationText(
        const ConversationNotification(
          chatId: 0,
          kind: NotificationConversationKind.lobby,
          unread: 5,
        ),
      ),
      '5 new in the lobby',
    );
    expect(conversationText(dm(1, 1)), '1 new message');
    // Neither names a sender: the summary has none to name.
    final all = shown.lines.map((l) => l.text).join(' ').toLowerCase();
    expect(all.contains('alice') || all.contains('bob'), isFalse);
  });

  // spec: app-ui/type-aware-notification-wording
  test('the iPhone line is the Android lines on one line: "2 new DMs, 1 new '
      'in Work"', () {
    final shown = renderNotifications(
      summary([dm(1, 1), dm(2, 1), group(100, 'Work', 1)]),
      NotificationDetailLevel.more,
    );
    expect(shown.combined, '2 new DMs, 1 new in Work');
  });

  // spec: app-ui/type-aware-notification-wording
  // spec: local-notifications/admin-event-notifications-are-fixed-content-free-phrases
  test('an admin-event is a fixed phrase naming the group', () {
    AdminEventNotification event(
      AdminEventType type, [
      NotificationConversationKind kind =
          NotificationConversationKind.namedGroup,
      String? name = 'Work',
    ]) => AdminEventNotification(id: 1, event: type, kind: kind, name: name);
    expect(
      adminEventText(event(AdminEventType.removed)),
      'You were removed from Work',
    );
    expect(adminEventText(event(AdminEventType.closed)), 'Work was closed');
    expect(
      adminEventText(event(AdminEventType.added)),
      'You were added to Work',
    );
    expect(
      adminEventText(
        event(
          AdminEventType.added,
          NotificationConversationKind.namedGroup,
          null,
        ),
      ),
      'You were added to a new group',
    );
    expect(
      adminEventText(
        event(AdminEventType.closed, NotificationConversationKind.dm, null),
      ),
      'A chat was closed',
    );
  });

  // spec: local-notifications/respect-the-notification-detail-level
  test('Basic ignores the summary\'s detail; More renders it', () {
    final work = summary([group(100, 'Work', 3)]);
    final basic = renderNotifications(work, NotificationDetailLevel.basic);
    expect(basic.lines.map((l) => l.text), ['New message']);
    expect(basic.combined, 'New message');
    expect(basic.badge, 0, reason: 'a number would say more than Basic does');
    expect(basic.singleChatId, isNull);
    final more = renderNotifications(work, NotificationDetailLevel.more);
    expect(more.lines.map((l) => l.text), ['3 new in Work']);
    expect(more.singleChatId, 100);
    expect(more.badge, 3);
    // Nothing to notify about: nothing, at either level.
    for (final level in NotificationDetailLevel.values) {
      expect(renderNotifications(summary([]), level).isEmpty, isTrue);
    }
  });

  // spec: app-ui/notification-detail-level
  // spec: local-notifications/respect-the-notification-detail-level
  test('a data wake at Basic posts only "New message", at More the count and '
      'type', () async {
    final work = summary([group(100, 'Work', 3)]);
    var (core, push, service) = await woken(
      summary: work,
      level: NotificationDetailLevel.basic,
    );
    await push.wake();
    expect(push.shown.single.lines.map((l) => l.text), ['New message']);
    expect(core.calls, containsAllInOrder(['start', 'stop']));
    await service.dispose();

    (core, push, service) = await woken(summary: work);
    await push.wake();
    expect(push.shown.single.lines.map((l) => l.text), ['3 new in Work']);
    await service.dispose();
  });

  // spec: app-ui/notification-detail-level
  test('a wake with nothing unread posts nothing, and one in front of the '
      'user posts nothing', () async {
    var (core, push, service) = await woken(summary: summary([]));
    await push.wake();
    expect(push.shown, isEmpty);
    await service.dispose();

    (core, push, service) = await woken(
      summary: summary([group(100, 'Work', 3)]),
      foreground: true,
    );
    await push.wake();
    expect(push.shown, isEmpty);
    expect(core.calls, isNot(contains('start')));
    await service.dispose();
  });

  // spec: local-notifications/admin-event-notifications-are-fixed-content-free-phrases
  test(
    'a wake for a removal posts the fixed phrase once, then forgets it',
    () async {
      final removed = summary([], [
        const AdminEventNotification(
          id: 7,
          event: AdminEventType.removed,
          kind: NotificationConversationKind.namedGroup,
          name: 'Work',
        ),
      ]);
      final (core, push, service) = await woken(summary: removed);
      await push.wake();
      expect(push.shown.single.lines.map((l) => l.text), [
        'You were removed from Work',
      ]);
      expect(core.args['clearAdminEvents'], 7);
      await push.wake();
      expect(push.shown, hasLength(1), reason: 'nothing left to say');
      await service.dispose();
    },
  );

  // spec: app-ui/notification-detail-level
  testWidgets('Notification detail starts at Basic; Bob picks More', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(800, 2400);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final core = FakeCoreService(storedUser: bob);
    await openHome(tester, core);
    await tap(tester, find.byKey(const Key('tab-profile')));
    expect(find.text('Notification detail'), findsOneWidget);
    SegmentedButton<NotificationDetailLevel> control() =>
        tester.widget(find.byKey(const Key('notification-detail')));
    expect(control().selected, {NotificationDetailLevel.basic});
    expect(find.textContaining('Shows only “New message”'), findsOneWidget);

    await tap(tester, find.text('More'));
    expect(core.args['setNotificationDetail'], NotificationDetailLevel.more);
    expect(control().selected, {NotificationDetailLevel.more});
    expect(find.textContaining('3 new in Work'), findsOneWidget);
  });

  // spec: app-ui/a-notification-opens-its-conversation
  testWidgets('Bob taps "3 new in Work": the Work chat opens and its '
      'notification is cleared', (tester) async {
    final core = FakeCoreService(storedUser: bob)
      ..groupChats = [
        const ChatSummary(
          chatId: 100,
          kind: ChatKindInfo.group(name: 'Work', closed: false),
          unread: 3,
          unseenMentions: 0,
          muted: false,
        ),
      ]
      ..groupMemberDevices = {100: []};
    final push = FakePush();
    await tester.pumpWidget(VyajenaApp(core: core, push: push));
    await core.open();
    await settle(tester);
    expect(find.byKey(const ValueKey('chat-100')), findsOneWidget);

    push.tap(100);
    await settle(tester);
    expect(push.clearedConversations, [100]);
    expect(find.byKey(const Key('composer')), findsOneWidget);
    expect(find.text('Work'), findsWidgets);
  });

  // spec: app-ui/a-notification-opens-its-conversation
  testWidgets('a notification that launched the app opens its chat once the '
      'core is open; an admin-event opens the chat list', (tester) async {
    final core = FakeCoreService(storedUser: bob)
      ..groupChats = [
        const ChatSummary(
          chatId: 100,
          kind: ChatKindInfo.group(name: 'Work', closed: false),
          unread: 1,
          unseenMentions: 0,
          muted: false,
        ),
      ]
      ..groupMemberDevices = {100: []};
    final push = FakePush()..launchTap = const NotificationTap(chatId: 100);
    await tester.pumpWidget(VyajenaApp(core: core, push: push));
    await core.open();
    await settle(tester);
    expect(find.byKey(const Key('composer')), findsOneWidget);

    // An admin-event names no chat: back to the chat list.
    push.tap(null);
    await settle(tester);
    expect(find.byKey(const Key('composer')), findsNothing);
    expect(find.byKey(const ValueKey('chat-100')), findsOneWidget);
    expect(push.clearedConversations, [100]);
  });
}
