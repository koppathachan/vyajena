// app-ui: Chat list; Admin screens; Named group screen (change
// `lobby-multi-group`).
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/core/core_service.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

const carol = UserRef(username: 'carol', serverId: 'chat');

ChatSummary group(
  int id,
  String? name, {
  bool closed = false,
  int unread = 0,
}) => ChatSummary(
  chatId: id,
  kind: ChatKindInfo.group(name: name, closed: closed),
  unread: unread,
  unseenMentions: 0,
  muted: false,
);

FakeCoreService adminWithGroups() => FakeCoreService(storedUser: akhil)
  ..admin = true
  ..memberDevices = const [
    MemberDevice(user: akhil, device: '01'),
    MemberDevice(user: alice, device: '02'),
    MemberDevice(user: bob, device: '03'),
    MemberDevice(user: carol, device: '04'),
  ]
  ..displayNames = {
    'alice@chat': 'Ali (alice)',
    'bob@chat': 'Bobby (bob)',
    'carol@chat': 'carol',
    'akhil@chat': 'akhil',
  };

FakeCoreService aliceInWeekend() => FakeCoreService(storedUser: alice)
  ..memberDevices = const [
    MemberDevice(user: akhil, device: '01'),
    MemberDevice(user: alice, device: '02'),
    MemberDevice(user: bob, device: '03'),
  ]
  ..displayNames = {'bob@chat': 'Bobby (bob)', 'akhil@chat': 'akhil'}
  ..groupChats = [group(100, 'Weekend', unread: 2)]
  ..groupMemberDevices = {
    100: const [
      MemberDevice(user: akhil, device: '01'),
      MemberDevice(user: alice, device: '02'),
    ],
  };

void main() {
  // spec: app-ui/chat-list
  testWidgets('the lobby shows it is fixed: no rename, no close, even for the '
      'admin', (tester) async {
    await openHome(tester, adminWithGroups());
    expect(find.text('lobby'), findsOneWidget);
    await enterGroup(tester);
    await tap(tester, find.byTooltip('Menu'));
    for (final key in ['lobby-rename-disabled', 'lobby-close-disabled']) {
      final item = tester.widget<PopupMenuItem<dynamic>>(
        find.byKey(ValueKey(key)),
      );
      expect(item.enabled, isFalse);
    }
    expect(find.text('Rename group'), findsNothing);
    expect(find.text('Close group'), findsNothing);
  });

  // spec: app-ui/chat-list
  testWidgets('lobby first labelled "lobby", then the group by name and the '
      'DM, each with its own unread mark', (tester) async {
    final core = aliceInWeekend()
      ..dmChats = [
        ChatSummary(
          chatId: 1,
          kind: const ChatKindInfo.dm(peer: bob, waiting: false, closed: false),
          unread: 1,
          unseenMentions: 0,
          muted: false,
        ),
      ];
    await openHome(tester, core);
    final rows = tester
        .widgetList<ListTile>(find.byType(ListTile))
        .map((t) => (t.title as Text).data)
        .toList();
    expect(rows, ['lobby', 'Weekend', 'Bobby (bob)']);
    expect(find.byKey(const ValueKey('chat-unread-100')), findsOneWidget);
    expect(find.byKey(const ValueKey('chat-unread-1')), findsOneWidget);
  });

  // spec: app-ui/chat-list
  testWidgets('a closed group is shown greyed with its banner text', (
    tester,
  ) async {
    final core = aliceInWeekend()
      ..groupChats = [group(100, 'Weekend', closed: true)];
    await openHome(tester, core);
    expect(find.text('This group was closed'), findsOneWidget);
  });

  // spec: app-ui/named-group-screen
  testWidgets('a named group is titled with its name and a placeholder until '
      'named', (tester) async {
    final core = aliceInWeekend()
      ..groupChats = [group(100, 'Weekend'), group(101, null)];
    await openHome(tester, core);
    await tap(tester, find.byKey(const ValueKey('chat-100')));
    expect(
      find.descendant(of: find.byType(AppBar), matching: find.text('Weekend')),
      findsOneWidget,
    );
    await tester.pageBack();
    await settle(tester);
    await tap(tester, find.byKey(const ValueKey('chat-101')));
    expect(
      find.descendant(
        of: find.byType(AppBar),
        matching: find.text('Unnamed group'),
      ),
      findsOneWidget,
    );
  });

  // spec: app-ui/named-group-screen
  testWidgets('Bob, a non-admin, sees no add, remove, rename or close in a '
      'group', (tester) async {
    final core = aliceInWeekend();
    await openHome(tester, core);
    await tap(tester, find.byKey(const ValueKey('chat-100')));
    await tap(tester, find.byKey(const Key('group-menu')));
    expect(find.text('Members'), findsOneWidget);
    for (final entry in [
      'Add member',
      'Remove member',
      'Rename group',
      'Close group',
    ]) {
      expect(find.text(entry), findsNothing);
    }
  });

  // spec: app-ui/named-group-screen
  testWidgets('a closed group shows its history, the banner and no composer', (
    tester,
  ) async {
    final core = aliceInWeekend()
      ..groupChats = [group(100, 'Weekend', closed: true)];
    await openHome(tester, core);
    await tap(tester, find.byKey(const ValueKey('chat-100')));
    expect(find.byKey(const Key('group-closed')), findsOneWidget);
    expect(find.byKey(const Key('composer')), findsNothing);
  });

  // spec: app-ui/named-group-screen
  testWidgets('@everyone is offered in a named group, over the group\'s own '
      'members', (tester) async {
    final core = aliceInWeekend();
    await openHome(tester, core);
    await tap(tester, find.byKey(const ValueKey('chat-100')));
    await tester.enterText(find.byKey(const Key('composer')), '@');
    await settle(tester);
    expect(find.text('everyone'), findsWidgets);
    // Bob is in the lobby but not in this group: he is not offered.
    expect(find.textContaining('Bobby'), findsNothing);
  });

  // spec: app-ui/admin-group-screens
  testWidgets('the admin creates a group by picking members', (tester) async {
    final core = adminWithGroups();
    await openHome(tester, core);
    await tester.tap(find.byKey(const Key('new-chat-fab')));
    await settle(tester);
    await tap(tester, find.byKey(const Key('new-group')));
    await tester.enterText(find.byKey(const Key('new-group-name')), 'Weekend');
    await tap(tester, find.byKey(const ValueKey('member-bob')));
    await tap(tester, find.byKey(const ValueKey('member-carol')));
    expect(find.byKey(const ValueKey('member-akhil')), findsNothing);
    await tap(tester, find.byKey(const Key('create-group-confirm')));
    expect(core.args['createNamedGroup'], [
      'Weekend',
      [bob, carol],
    ]);
    // The new group's screen opens.
    expect(
      find.descendant(of: find.byType(AppBar), matching: find.text('Weekend')),
      findsOneWidget,
    );
  });

  // spec: app-ui/admin-group-screens
  testWidgets('the admin renames and closes a group, with a confirm for the '
      'close', (tester) async {
    final core = adminWithGroups()..groupChats = [group(100, 'Weekend')];
    await openHome(tester, core);
    await tap(tester, find.byKey(const ValueKey('chat-100')));

    await tap(tester, find.byKey(const Key('group-menu')));
    await tap(tester, find.text('Rename group'));
    await tester.enterText(find.byKey(const Key('group-rename-field')), 'Away');
    await tap(tester, find.byKey(const Key('group-rename-save')));
    expect(core.args['renameGroup'], [100, 'Away']);
    expect(
      find.descendant(of: find.byType(AppBar), matching: find.text('Away')),
      findsOneWidget,
    );

    await tap(tester, find.byKey(const Key('group-menu')));
    await tap(tester, find.text('Close group'));
    expect(core.calls, isNot(contains('closeGroup')));
    await tap(tester, find.byKey(const Key('group-close-confirm')));
    expect(core.args['closeGroup'], 100);
    expect(find.byKey(const Key('group-closed')), findsOneWidget);
  });

  // spec: app-ui/add-a-member-to-an-existing-named-group
  testWidgets('the admin adds an existing user to an existing named group '
      'without recreating it', (tester) async {
    final core = adminWithGroups()
      ..groupChats = [group(100, 'Weekend')]
      ..groupMemberDevices = {
        100: const [
          MemberDevice(user: akhil, device: '01'),
          MemberDevice(user: alice, device: '02'),
        ],
      };
    await openHome(tester, core);
    await tap(tester, find.byKey(const ValueKey('chat-100')));
    await tap(tester, find.byKey(const Key('group-menu')));
    await tap(tester, find.text('Add member'));
    // Bob and Carol are in the lobby but not in Weekend: they are offered.
    expect(find.byKey(const ValueKey('pick-bob')), findsOneWidget);
    expect(find.byKey(const ValueKey('pick-carol')), findsOneWidget);
    // Alice is already in the group and the admin is excluded.
    expect(find.byKey(const ValueKey('pick-alice')), findsNothing);
    expect(find.byKey(const ValueKey('pick-akhil')), findsNothing);
    await tap(tester, find.byKey(const ValueKey('pick-bob')));
    expect(core.args['addToGroup'], [100, bob]);
  });

  // spec: app-ui/admin-group-screens
  testWidgets('the lobby has no rename or close, and Bob has no create-group '
      'entry', (tester) async {
    final core = FakeCoreService(storedUser: bob);
    await openHome(tester, core);
    await tester.tap(find.byKey(const Key('new-chat-fab')));
    await settle(tester);
    expect(find.byKey(const Key('new-group')), findsNothing);
  });
}
