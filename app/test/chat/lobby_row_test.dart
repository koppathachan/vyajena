// app-ui: Lobby row styling (change lobby-distinct), in the Arcade look.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../shell/app_shell_test.dart' show aliceWithEverything;
import '../support/app.dart';

void main() {
  // spec: app-ui/lobby-row-in-the-arcade-look
  // spec: app-ui/lobby-row-styling
  testWidgets('the lobby row has the hash tile, an italic name, the system '
      'pill and a tint, and is first', (tester) async {
    await openHome(tester, aliceWithEverything());
    final tiles = tester.widgetList<ListTile>(find.byType(ListTile)).toList();
    expect(tileTitle(tiles.first), 'lobby');
    expect(find.byKey(const Key('lobby-hash-tile')), findsOneWidget);
    expect(find.byKey(const Key('lobby-system-pill')), findsOneWidget);
    expect(find.text('system'), findsOneWidget);
    expect(tiles.first.tileColor, isNotNull);
    final name = tester.widget<Text>(find.text('lobby'));
    expect(name.style?.fontStyle, FontStyle.italic);
    // The hash tile is a square, in the accent colour.
    final tile = tester.widget<Container>(
      find.byKey(const Key('lobby-hash-tile')),
    );
    final decoration = tile.decoration! as BoxDecoration;
    expect(decoration.borderRadius, isNull);
    expect(decoration.shape, BoxShape.rectangle);
  });

  // spec: app-ui/lobby-row-styling
  testWidgets('only the lobby is styled: a group and a DM are plain', (
    tester,
  ) async {
    await openHome(tester, aliceWithEverything());
    final tiles = tester.widgetList<ListTile>(find.byType(ListTile)).toList();
    expect(tiles.map(tileTitle), ['lobby', 'Weekend', 'Bobby (bob)']);
    for (final tile in tiles.skip(1)) {
      expect(tile.tileColor, isNull);
      expect(tile.title, isA<Text>());
      expect((tile.title! as Text).style?.fontStyle, isNot(FontStyle.italic));
    }
    expect(find.byKey(const Key('lobby-hash-tile')), findsOneWidget);
    expect(find.byKey(const Key('lobby-system-pill')), findsOneWidget);
  });
}
