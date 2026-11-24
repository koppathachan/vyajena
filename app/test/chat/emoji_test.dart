// app-ui: Emoji-only messages (jumbomoji).
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vyajena/chat/emoji.dart';

import '../support/app.dart';
import '../support/fake_core_service.dart';

void main() {
  group('isEmojiOnly', () {
    test('plain emoji and runs of them are emoji-only', () {
      expect(isEmojiOnly('👍'), isTrue);
      expect(isEmojiOnly('😀😀😀'), isTrue);
      expect(isEmojiOnly('🔥 🎉'), isTrue); // spaces allowed
      expect(isEmojiOnly('❤️'), isTrue); // with variation selector
      expect(isEmojiOnly('👍🏽'), isTrue); // with skin-tone modifier
      expect(isEmojiOnly('👨‍👩‍👧'), isTrue); // ZWJ family sequence
      expect(isEmojiOnly('⭐'), isTrue);
    });

    test('anything with letters, digits or other text is not', () {
      expect(isEmojiOnly('hi 👍'), isFalse);
      expect(isEmojiOnly('👍 ok'), isFalse);
      expect(isEmojiOnly('100'), isFalse);
      expect(isEmojiOnly('nope'), isFalse);
      expect(isEmojiOnly(''), isFalse);
      expect(isEmojiOnly('   '), isFalse);
    });
  });

  group('jumboEmojiSize', () {
    test('steps down as the count grows, largest for one or two', () {
      final one = jumboEmojiSize('👍')!;
      final three = jumboEmojiSize('😀😀😀')!;
      final five = jumboEmojiSize('😀😀😀😀😀')!;
      expect(one, greaterThan(three));
      expect(three, greaterThan(five));
      expect(five, greaterThan(16)); // still clearly larger than body text
    });

    test('a family ZWJ sequence counts as one emoji, so it is largest', () {
      expect(jumboEmojiSize('👨‍👩‍👧'), jumboEmojiSize('👍'));
    });

    test('long runs and ordinary text are not enlarged', () {
      expect(jumboEmojiSize('😀😀😀😀😀😀😀😀😀'), isNull); // over the cap
      expect(jumboEmojiSize('hello'), isNull);
      expect(jumboEmojiSize('great 👍'), isNull);
    });
  });

  testWidgets('an emoji-only message renders large (jumbomoji), a mixed one '
      'does not', (tester) async {
    final core = FakeCoreService(storedUser: alice);
    await openApp(tester, core);
    final jumbo = core.receive(bob, '🎉');
    final plain = core.receive(bob, 'hi 👍');
    await settle(tester);

    final jumboText = tester.widget<Text>(
      find.byKey(ValueKey('jumbo-${jumbo.id}')),
    );
    expect(jumboText.data, '🎉');
    expect(jumboText.style?.fontSize, greaterThan(24));
    // The mixed message is not drawn as a jumbo.
    expect(find.byKey(ValueKey('jumbo-${plain.id}')), findsNothing);
    expect(find.text('hi 👍'), findsOneWidget);
  });
}
