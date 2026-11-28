// app-ui: Lobby row styling (change lobby-distinct), in the Arcade look.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/app/arcade/arcade_border.dart';
import 'package:vyajena/app/arcade/tokens.dart';
import 'package:vyajena/shell/avatars.dart';

import '../shell/app_shell_test.dart' show aliceWithEverything;
import '../support/app.dart';

void main() {
  // spec: app-ui/lobby-row-in-the-arcade-look
  // spec: app-ui/lobby-row-styling
  testWidgets('the lobby row has the hash tile, an italic name, the system '
      'pill and a tint, and is first', (tester) async {
    await openHome(tester, aliceWithEverything());
    final tiles = tester.widgetList<ListTile>(find.byType(ListTile)).toList();
    expect(tileTitle(tiles.first), '#lobby');
    expect(find.byKey(const Key('lobby-hash-tile')), findsOneWidget);
    expect(find.byKey(const Key('lobby-system-pill')), findsOneWidget);
    expect(find.text('system'), findsOneWidget);
    expect(tiles.first.tileColor, isNotNull);
    final name = tester.widget<Text>(find.text('#lobby'));
    expect(name.style?.fontStyle, FontStyle.italic);
    // The hash tile is a hand-drawn square with the 2px border.
    final tile = tester.widget<Container>(
      find.byKey(const Key('lobby-hash-tile')),
    );
    final decoration = tile.decoration! as ShapeDecoration;
    final border = decoration.shape as ArcadeBorder;
    expect(border.width, arcadeBorderWidth);
  });

  // spec: app-ui/lobby-row-styling
  // spec: app-ui/groups-distinct-from-dms
  testWidgets('only the lobby is the system space; a group is a stack, a DM '
      'one tile', (tester) async {
    await openHome(tester, aliceWithEverything());
    final tiles = tester.widgetList<ListTile>(find.byType(ListTile)).toList();
    expect(tiles.map(tileTitle), ['#lobby', 'Weekend', 'Bobby (bob)']);
    for (final tile in tiles.skip(1)) {
      expect(tile.tileColor, isNull);
      expect(
        tester.widget<Text>(find.text(tileTitle(tile)!)).style?.fontStyle,
        isNot(FontStyle.italic),
      );
    }
    expect(find.byKey(const Key('lobby-hash-tile')), findsOneWidget);
    expect(find.byKey(const Key('lobby-system-pill')), findsOneWidget);
    // The group: a stacked avatar and a group mark; the DM: neither.
    final group = find.byKey(tiles[1].key!);
    final dm = find.byKey(tiles[2].key!);
    expect(
      find.descendant(of: group, matching: find.byType(GroupAvatar)),
      findsOneWidget,
    );
    expect(
      find.descendant(of: group, matching: find.byIcon(Icons.groups_outlined)),
      findsOneWidget,
    );
    expect(
      find.descendant(of: dm, matching: find.byType(GroupAvatar)),
      findsNothing,
    );
    expect(tiles[2].title, isA<Text>());
  });
}
